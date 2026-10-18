import csv
import json
import unittest
import tempfile
from contextlib import nullcontext, redirect_stdout
from io import StringIO
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import call, patch

from pubg_highlight_trim.cli import build_parser
from pubg_highlight_trim.models import EventDetection
from pubg_highlight_trim.game_languages import get_game_language_profile
from pubg_highlight_trim.ocr import OcrConfig
from pubg_highlight_trim.pipeline import (
    _apply_context_rules,
    _automatic_jobs,
    _effective_jobs,
    _candidate_csv_path,
    _effective_no_merge,
    _select_input_files,
    _validate_inputs,
    _merge_output_override,
    _merged_clip_plans,
    _ocr_config_for_source,
    _partition_too_early_detections,
    _prepare_output_paths,
    _progress_line,
    _record_detection,
    _record_skip,
    run,
    _scan_source_worker,
    _WORKER_OCR_BACKENDS,
    _use_fast_scan,
)


class PipelineMergeTests(unittest.TestCase):
    def _run_with_mocked_video_pipeline(self, args, detection):
        def write_trim(_src, out, _start, _length, _ffmpeg):
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(b"trim")
            return "mock-encoder"

        def write_merge(clips, final, _ffmpeg):
            final.parent.mkdir(parents=True, exist_ok=True)
            final.write_bytes(b"merged")
            list_path = final.with_suffix(".concat.txt")
            list_path.write_text("\n".join(str(clip) for clip in clips), encoding="utf-8")
            return list_path

        with patch("pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))), patch(
            "pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())
        ), patch("pubg_highlight_trim.pipeline.duration_sec", return_value=60.0), patch(
            "pubg_highlight_trim.pipeline.detect_ocr_events", return_value=detection if isinstance(detection, list) else [detection]
        ) as detect_events, patch("pubg_highlight_trim.pipeline.trim_clip", side_effect=write_trim) as trim_clip, patch(
            "pubg_highlight_trim.pipeline.concat_clips", side_effect=write_merge
        ) as concat_clips:
            code = run(args)
        self.assertEqual(detect_events.call_count, len(args.files or [args.input]))
        self.assertTrue(all(call.args[5].target == args.target for call in detect_events.call_args_list))
        return code, trim_clip, concat_clips

    def test_run_deduplicates_overlapping_sources_before_trim(self):
        import numpy as np

        for later_complete, early_seek_mismatch in ((False, False), (True, False), (False, True)):
            with self.subTest(later_complete=later_complete, early_seek_mismatch=early_seek_mismatch), tempfile.TemporaryDirectory() as tmp:
                # run() resolves input paths (including Windows short-name TEMP paths).
                root = Path(tmp).resolve()
                first, second = root / "a.mp4", root / "b.mp4"
                first.write_bytes(b"original-a")
                second.write_bytes(b"original-b")
                outdir = root / "output"
                args = build_parser().parse_args([
                    "--files", str(first), str(second), "--game-lang", "en",
                    "-o", str(outdir), "--jobs", "1", "-y",
                ])
                times = [2.0, 30.5] if later_complete else [30.5, 2.0]
                detections = [EventDetection(t + 1, t, "death", "ocr", target="self-death",
                                             event_key="self-death:knock:enemy") for t in times]

                class Frames:
                    def __init__(self, path, *_args, **_kwargs):
                        self.event = times[0 if path == first else 1]
                    def __enter__(self):
                        return self
                    def __exit__(self, *_args):
                        pass
                    def read_at(self, sec):
                        seed = round((sec - self.event + 10) * 1000)
                        if early_seek_mismatch and self.event == 2 and sec < 2:
                            seed += 100
                        rng = np.random.default_rng(seed)
                        return rng.integers(0, 256, (48, 64, 3), dtype=np.uint8)

                with patch("pubg_highlight_trim.pipeline.FfmpegFrameSource", Frames):
                    code, trim, concat = self._run_batch_fixture(args, iter(detections))
                self.assertEqual(code, 0)
                self.assertEqual(trim.call_count, 1)
                kept = second if later_complete else first
                self.assertEqual(trim.call_args.args[0], kept)
                self.assertEqual(trim.call_args.args[2:4], (25.5, 6.0))
                self.assertEqual(len(concat.call_args.args[0]), 1)
                entries = (outdir / "pubg_highlight_trim_merged.concat.txt").read_text(encoding="utf-8").splitlines()
                self.assertEqual(len(entries), 1)
                with (outdir / "检测与裁剪记录.csv").open(encoding="utf-8-sig", newline="") as handle:
                    rows = list(csv.DictReader(handle))
                self.assertEqual(sorted(row["Status"] for row in rows), ["included", "skipped"])
                skipped = next(row for row in rows if row["Status"] == "skipped")
                self.assertEqual(skipped["Method"], "skipped-cross-source-duplicate")
                self.assertEqual(skipped["Output"], "")
                self.assertEqual(first.read_bytes(), b"original-a")
                self.assertEqual(second.read_bytes(), b"original-b")

    def test_run_keeps_events_without_changing_cross_source_overlap_evidence(self):
        import numpy as np

        for evidence in ("different-game", "black", "static", "hud-only", "missing", "decode-error",
                         "different-key", "missing-key", "same-source", "extra-event"):
            with self.subTest(evidence=evidence), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp).resolve()
                first, second = root / "a.mp4", root / "b.mp4"
                first.touch()
                second.touch()
                outdir = root / "output"
                single = evidence == "same-source"
                args = build_parser().parse_args([
                    "--files", str(first), *([] if single else [str(second)]),
                    "--game-lang", "en", "-o", str(outdir), "--jobs", "1", "--merge", "-y",
                ])
                def event(sec, key="enemy"):
                    return EventDetection(60, sec, "death", "ocr", target="self-death",
                                          event_key=f"self-death:knock:{key}" if key else "")
                first_events = [event(30.5)]
                second_events = [event(2, "another" if evidence == "different-key" else
                                        "" if evidence == "missing-key" else "enemy")]
                if single:
                    first_events.append(event(45))
                if evidence == "extra-event":
                    second_events.append(event(3, "unique"))

                class Frames:
                    def __init__(self, path, *_args, **_kwargs):
                        self.path = path
                    def __enter__(self):
                        return self
                    def __exit__(self, *_args):
                        pass
                    def read_at(self, sec):
                        if evidence == "decode-error":
                            raise RuntimeError("decoder unavailable")
                        if evidence == "missing":
                            return None
                        offset = sec - (30.5 if self.path == first else 2)
                        seed = round((offset + 10) * 1000)
                        if evidence == "different-game" and self.path == second:
                            seed += 10
                        if evidence in ("static", "hud-only"):
                            seed = 123
                        frame = np.random.default_rng(seed).integers(0, 256, (48, 64, 3), dtype=np.uint8)
                        if evidence == "black":
                            frame[:] = 0
                        if evidence == "hud-only":
                            frame[:4] = round((offset + 10) * 10)
                        return frame

                queue = iter([first_events] if single else [first_events, second_events])
                with patch("pubg_highlight_trim.pipeline.FfmpegFrameSource", Frames):
                    code, trim, concat = self._run_batch_fixture(args, queue)
                self.assertEqual(code, 0)
                self.assertEqual(trim.call_count, 2)
                self.assertEqual(len(concat.call_args.args[0]), 2)
                self.assertEqual(len(list((outdir / "clips").glob("*.mp4"))), 2)
                with (outdir / "检测与裁剪记录.csv").open(encoding="utf-8-sig", newline="") as handle:
                    rows = list(csv.DictReader(handle))
                self.assertEqual(sum(row["Status"] == "included" for row in rows), 2)
                skipped = [row for row in rows if row["Status"] == "skipped"]
                self.assertEqual(len(skipped), 1 if evidence == "extra-event" else 0)
                if evidence == "extra-event":
                    self.assertTrue(any(row["EventKeys"] == "self-death:knock:unique"
                                        for row in rows if row["Status"] == "included"))

    def _run_batch_fixture(self, args, queue):
        def trim(src, out, start, length, ffmpeg):
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(b"trim")
            return "mock-encoder"
        def concat(clips, final, ffmpeg):
            final.write_bytes(b"merged")
            path = final.with_suffix(".concat.txt")
            path.write_text("\n".join(map(str, clips)), encoding="utf-8")
            return path
        def detect(*_args, **_kwargs):
            result = next(queue)
            return result if isinstance(result, list) else [result]
        with patch("pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=("ffmpeg", "ffprobe")), patch(
            "pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())
        ), patch("pubg_highlight_trim.pipeline.duration_sec", return_value=60.0), patch(
            "pubg_highlight_trim.pipeline.detect_ocr_events", side_effect=detect
        ), patch("pubg_highlight_trim.pipeline.trim_clip", side_effect=trim) as trim_mock, patch(
            "pubg_highlight_trim.pipeline.concat_clips", side_effect=concat
        ) as concat_mock:
            code = run(args)
        return code, trim_mock, concat_mock

    def test_run_places_trimmed_clips_under_clips_and_merge_at_output_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first = root / "first.mp4"
            second = root / "second.mp4"
            output_dir = root / "output"
            first.touch()
            second.touch()
            args = build_parser().parse_args(
                [
                    "--files",
                    str(second),
                    str(first),
                    "--game-lang",
                    "en",
                    "--output-dir",
                    str(output_dir),
                    "-y",
                    "--jobs",
                    "1",
                ]
            )

            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            code, _trim_clip, _concat_clips = self._run_with_mocked_video_pipeline(args, detection)

            self.assertEqual(code, 0)
            self.assertEqual(sorted(path.name for path in (output_dir / "clips").glob("*.mp4")), [
                "001_own-kill_first.mp4",
                "002_own-kill_second.mp4",
            ])
            self.assertEqual([path.name for path in output_dir.glob("*.mp4")], ["pubg_highlight_trim_merged.mp4"])
            concat_list = output_dir / "pubg_highlight_trim_merged.concat.txt"
            self.assertTrue(concat_list.is_file())
            self.assertEqual(
                concat_list.read_text(encoding="utf-8").splitlines(),
                [
                    str(output_dir / "clips" / "001_own-kill_first.mp4"),
                    str(output_dir / "clips" / "002_own-kill_second.mp4"),
                ],
            )

            with (output_dir / "检测与裁剪记录.csv").open(encoding="utf-8-sig", newline="") as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual([Path(row["Output"]).parent for row in rows], [output_dir / "clips"] * 2)
            self.assertTrue(all(Path(row["Output"]).is_file() for row in rows))

            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["output_dir"], str(output_dir))
            self.assertEqual(summary["merge_output"], str(output_dir / "pubg_highlight_trim_merged.mp4"))
            self.assertEqual(summary["concat_list"], str(output_dir / "pubg_highlight_trim_merged.concat.txt"))
            self.assertEqual(summary["csv"], str(output_dir / "检测与裁剪记录.csv"))
            self.assertTrue(Path(summary["csv"]).is_file())
            self.assertTrue(Path(summary["concat_list"]).is_file())

    def test_split_run_keeps_overlapping_targets_in_separate_outputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source.mp4"
            source.touch()
            outdir = root / "output"
            args = build_parser().parse_args([
                str(source), "--game-lang", "en", "-o", str(outdir),
                "--merge", "--split-merge-by-target", "--jobs", "1",
            ])
            detections = [
                EventDetection(60.0, 30.0, "kill", "ocr", target="own-kill"),
                EventDetection(60.0, 31.0, "death", "ocr", target="self-death"),
                EventDetection(60.0, 32.0, "kill", "ocr", target="own-kill"),
            ]
            code, trim, concat = self._run_with_mocked_video_pipeline(args, detections)
            self.assertEqual(code, 0)
            self.assertEqual(trim.call_count, 2)
            self.assertEqual(concat.call_count, 2)
            summary = json.loads((outdir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual([item["target"] for item in summary["merge_outputs"]], ["self-death", "own-kill"])
            for item in summary["merge_outputs"]:
                self.assertTrue(Path(item["path"]).is_file())
                entries = Path(item["concat_list"]).read_text(encoding="utf-8").splitlines()
                self.assertEqual(len(entries), 1)
                self.assertIn(item["target"], Path(entries[0]).name)
            self.assertEqual(summary["merge_output"], summary["merge_outputs"][0]["path"])
            self.assertEqual(summary["concat_list"], summary["merge_outputs"][0]["concat_list"])
            with (outdir / "检测与裁剪记录.csv").open(encoding="utf-8-sig", newline="") as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual({row["Target"] for row in rows}, {"self-death", "own-kill"})
            self.assertEqual(sorted(row["EventCount"] for row in rows), ["1", "2"])

    def test_split_outputs_respect_missing_targets_and_merge_switches(self):
        scenarios = [
            ([], [], 2),
            ([], [EventDetection(60.0, None, "no-event", "ocr")], 2),
            ([], [EventDetection(60.0, 30.0, "kill", "ocr", target="own-kill")], 0),
            (["--no-merge"], [EventDetection(60.0, 30.0, "kill", "ocr", target="own-kill")], 0),
            (["--dry-run"], [EventDetection(60.0, 30.0, "kill", "ocr", target="own-kill")], 0),
        ]
        for switches, detections, expected_code in scenarios:
            with self.subTest(switches=switches, events=len(detections)), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                source = root / "source.mp4"
                source.touch()
                outdir = root / "output"
                args = build_parser().parse_args([
                    str(source), "--game-lang", "en", "-o", str(outdir),
                    "--split-merge-by-target", "--jobs", "1", *switches,
                ])
                if "--no-merge" not in switches:
                    args.merge = True
                code, trim, concat = self._run_with_mocked_video_pipeline(args, detections)
                self.assertEqual(code, expected_code)
                summary = json.loads((outdir / "summary.json").read_text(encoding="utf-8"))
                if expected_code == 2 or switches:
                    self.assertEqual(summary["merge_outputs"], [])
                    self.assertEqual(summary["merge_output"], "")
                    self.assertEqual(summary["concat_list"], "")
                    concat.assert_not_called()
                    self.assertEqual(list(outdir.glob("*.concat.txt")), [])
                else:
                    self.assertEqual([item["target"] for item in summary["merge_outputs"]], ["own-kill"])
                if "--dry-run" in switches:
                    trim.assert_not_called()

    def test_explicit_split_paths_preserve_base_and_handle_existing_outputs(self):
        for overwrite in (False, True):
            with self.subTest(overwrite=overwrite), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                source = root / "source.mp4"
                source.touch()
                base = root / "自定义.mp4"
                base.write_bytes(b"base")
                death = root / "自定义_self-death.mp4"
                kill = root / "自定义_own-kill.mp4"
                death.write_bytes(b"old")
                kill.write_bytes(b"old")
                outdir = root / "output"
                args = build_parser().parse_args([
                    str(source), "--game-lang", "en", "-o", str(outdir),
                    "--merge", str(base), "--split-merge-by-target", "--jobs", "1",
                    *(["-y"] if overwrite else []),
                ])
                detections = [
                    EventDetection(60.0, 30.0, "kill", "ocr", target="own-kill"),
                    EventDetection(60.0, 31.0, "death", "ocr", target="self-death"),
                ]
                self._run_with_mocked_video_pipeline(args, detections)
                summary = json.loads((outdir / "summary.json").read_text(encoding="utf-8"))
                suffix = "" if overwrite else "_1"
                self.assertEqual([Path(item["path"]).name for item in summary["merge_outputs"]], [
                    f"自定义_self-death{suffix}.mp4", f"自定义_own-kill{suffix}.mp4",
                ])
                self.assertEqual(base.read_bytes(), b"base")
                self.assertEqual(death.read_bytes(), b"merged" if overwrite else b"old")
                self.assertEqual(kill.read_bytes(), b"merged" if overwrite else b"old")

    def test_split_paths_cannot_overwrite_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "custom_own-kill.mp4"
            source.write_bytes(b"original")
            args = build_parser().parse_args([
                str(source), "--game-lang", "en", "-o", str(root / "output"),
                "--merge", str(root / "custom.mp4"), "--split-merge-by-target", "--jobs", "1", "-y",
            ])
            with self.assertRaisesRegex(SystemExit, "must not overwrite an input file"):
                self._run_with_mocked_video_pipeline(
                    args, EventDetection(60.0, 30.0, "kill", "ocr", target="own-kill")
                )
            self.assertEqual(source.read_bytes(), b"original")

    def test_split_default_names_follow_windows_ui_language(self):
        for language_id, expected in [(0x0804, ["被击杀集锦.mp4", "击杀集锦.mp4"]),
                                      (0x0409, ["self-death.mp4", "own-kill.mp4"])]:
            with self.subTest(language_id=language_id), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                source = root / "源视频.mp4"
                source.touch()
                outdir = root / "输出"
                args = build_parser().parse_args([
                    str(source), "--game-lang", "en", "-o", str(outdir),
                    "--merge", "--split-merge-by-target", "--jobs", "1",
                ])
                with patch("pubg_highlight_trim.pipeline.ctypes.windll", create=True) as windll:
                    windll.kernel32.GetUserDefaultUILanguage.return_value = language_id
                    self._run_with_mocked_video_pipeline(args, [
                        EventDetection(60.0, 30.0, "kill", "ocr", target="own-kill"),
                        EventDetection(60.0, 31.0, "death", "ocr", target="self-death"),
                    ])
                summary = json.loads((outdir / "summary.json").read_text(encoding="utf-8"))
                # On non-Windows the documented fallback is English.
                import os
                names = expected if os.name == "nt" else ["self-death.mp4", "own-kill.mp4"]
                self.assertEqual([Path(item["path"]).name for item in summary["merge_outputs"]], names)

    def test_single_target_ignores_split_and_all_mode_still_mixes(self):
        for target in ("self-death", "own-kill", "both"):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                source = root / "source.mp4"
                source.touch()
                outdir = root / "output"
                args = build_parser().parse_args([
                    str(source), "--game-lang", "en", "-o", str(outdir),
                    "--target", target, "--merge", "--jobs", "1",
                    *(["--split-merge-by-target"] if target != "both" else []),
                ])
                detections = [EventDetection(60.0, 30.0, "event", "ocr", target=target)]
                if target == "both":
                    detections = [
                        EventDetection(60.0, 30.0, "kill", "ocr", target="own-kill"),
                        EventDetection(60.0, 31.0, "death", "ocr", target="self-death"),
                    ]
                _, trim, concat = self._run_with_mocked_video_pipeline(args, detections)
                self.assertEqual(trim.call_count, 1)
                self.assertEqual(concat.call_count, 1)
                summary = json.loads((outdir / "summary.json").read_text(encoding="utf-8"))
                self.assertEqual(summary["merge_outputs"], [{
                    "target": target, "path": str(outdir / "source_pubg_trim.mp4"),
                    "concat_list": str(outdir / "source_pubg_trim.concat.txt"),
                }])

    def test_run_dry_run_does_not_create_clips_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source.mp4"
            output_dir = root / "output"
            source.touch()
            args = build_parser().parse_args(
                [
                    str(source),
                    "--game-lang",
                    "en",
                    "--output-dir",
                    str(output_dir),
                    "--dry-run",
                    "--jobs",
                    "1",
                    "-y",
                ]
            )
            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            code, trim_clip, concat_clips = self._run_with_mocked_video_pipeline(args, detection)

            self.assertEqual(code, 0)
            self.assertFalse((output_dir / "clips").exists())
            self.assertEqual(list(output_dir.glob("*.mp4")), [])
            trim_clip.assert_not_called()
            concat_clips.assert_not_called()

            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["merge_output"], "")
            self.assertEqual(summary["concat_list"], "")

    def test_run_explicit_merge_path_keeps_trimmed_clips_under_output_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source.mp4"
            output_dir = root / "output"
            merged = root / "merged" / "custom.mp4"
            source.touch()
            args = build_parser().parse_args(
                [
                    str(source),
                    "--game-lang",
                    "en",
                    "--output-dir",
                    str(output_dir),
                    "--merge",
                    str(merged),
                    "--jobs",
                    "1",
                    "-y",
                ]
            )
            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")

            code, _trim_clip, _concat_clips = self._run_with_mocked_video_pipeline(args, detection)

            self.assertEqual(code, 0)
            self.assertTrue((output_dir / "clips" / "001_own-kill_source.mp4").is_file())
            self.assertTrue(merged.is_file())
            self.assertFalse((output_dir / "custom.mp4").exists())
            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["merge_output"], str(merged))
            self.assertEqual(summary["concat_list"], str(merged.with_suffix(".concat.txt")))

    def test_run_single_file_defaults_to_no_merge_and_writes_trimmed_clips(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source.mp4"
            output_dir = root / "output"
            source.touch()
            args = build_parser().parse_args(
                [
                    str(source),
                    "--game-lang",
                    "en",
                    "--output-dir",
                    str(output_dir),
                    "--jobs",
                    "1",
                    "-y",
                ]
            )
            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")

            code, _trim_clip, concat_clips = self._run_with_mocked_video_pipeline(args, detection)

            self.assertEqual(code, 0)
            self.assertTrue((output_dir / "clips" / "001_own-kill_source.mp4").is_file())
            self.assertEqual(list(output_dir.glob("*.mp4")), [])
            concat_clips.assert_not_called()
            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertFalse(summary["merge"])
            self.assertEqual(summary["merge_output"], "")

    def test_run_uses_unique_output_dir_for_clips(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source.mp4"
            output_dir = root / "output"
            source.touch()
            output_dir.mkdir()
            args = build_parser().parse_args(
                [
                    str(source),
                    "--game-lang",
                    "en",
                    "--output-dir",
                    str(output_dir),
                    "--jobs",
                    "1",
                ]
            )
            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            code, _trim_clip, _concat_clips = self._run_with_mocked_video_pipeline(args, detection)

            unique_output = root / "output_1"
            self.assertEqual(code, 0)
            self.assertTrue((unique_output / "clips" / "001_own-kill_source.mp4").is_file())
            self.assertFalse((output_dir / "clips").exists())
            summary = json.loads((unique_output / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["output_dir"], str(unique_output))

    def test_run_overwrite_clears_old_root_trimmed_clips(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source.mp4"
            output_dir = root / "output"
            source.touch()
            output_dir.mkdir()
            (output_dir / "001_old.mp4").write_bytes(b"old")
            (output_dir / "clips").mkdir()
            (output_dir / "clips" / "001_old.mp4").write_bytes(b"old")
            args = build_parser().parse_args(
                [
                    str(source),
                    "--game-lang",
                    "en",
                    "--output-dir",
                    str(output_dir),
                    "--jobs",
                    "1",
                    "-y",
                ]
            )
            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            code, _trim_clip, _concat_clips = self._run_with_mocked_video_pipeline(args, detection)

            self.assertEqual(code, 0)
            self.assertFalse((output_dir / "001_old.mp4").exists())
            self.assertFalse((output_dir / "clips" / "001_old.mp4").exists())
            self.assertTrue((output_dir / "clips" / "001_own-kill_source.mp4").is_file())

    def test_progress_line_is_machine_readable_and_path_free(self):
        self.assertEqual(
            _progress_line("scan", 3, 12, 2),
            'PROGRESS {"phase":"scan","current":3,"total":12,"workers":2}',
        )

    def test_frozen_scan_worker_suppresses_process_output(self):
        language = get_game_language_profile("en")
        _WORKER_OCR_BACKENDS.clear()
        with patch("pubg_highlight_trim.pipeline.get_game_language_profile", return_value=language), patch(
            "pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())
        ) as load_backend, patch("pubg_highlight_trim.pipeline.duration_sec", return_value=60.0), patch(
            "pubg_highlight_trim.pipeline.detect_ocr_events", return_value=[]
        ), patch(
            "pubg_highlight_trim.pipeline.suppress_process_output", side_effect=lambda _enabled: nullcontext()
        ) as suppress, patch("pubg_highlight_trim.pipeline.sys.frozen", True, create=True):
            _scan_source_worker(Path("video.mp4"), Path("ffprobe.exe"), "en", [], OcrConfig())
            _scan_source_worker(Path("video2.mp4"), Path("ffprobe.exe"), "en", [], OcrConfig())

        self.assertEqual(load_backend.call_count, 1)
        self.assertEqual(suppress.call_args_list, [call(True), call(True), call(True)])
        _WORKER_OCR_BACKENDS.clear()

    def test_scan_worker_threads_ffmpeg_into_frame_source_factory(self):
        from pubg_highlight_trim.frame_source import FfmpegFrameSource

        captured = {}

        def fake_detect(*_args, **kwargs):
            captured.update(kwargs)
            return []

        _WORKER_OCR_BACKENDS.clear()
        with patch("pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())), patch(
            "pubg_highlight_trim.pipeline.duration_sec", return_value=60.0
        ), patch("pubg_highlight_trim.pipeline.detect_ocr_events", side_effect=fake_detect):
            _scan_source_worker(Path("video.mp4"), "ffprobe.exe", "en", [], OcrConfig(), False, ffmpeg="ffmpeg.exe")
        _WORKER_OCR_BACKENDS.clear()

        factory = captured["frame_source_factory"]
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n") as probe:
            source = factory(Path("video.mp4"))

        self.assertIsInstance(source, FfmpegFrameSource)
        self.assertEqual(probe.call_args[0][0][0], "ffprobe.exe")

    def test_scan_worker_without_ffmpeg_keeps_default_factory(self):
        captured = {}

        def fake_detect(*_args, **kwargs):
            captured.update(kwargs)
            return []

        _WORKER_OCR_BACKENDS.clear()
        with patch("pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())), patch(
            "pubg_highlight_trim.pipeline.duration_sec", return_value=60.0
        ), patch("pubg_highlight_trim.pipeline.detect_ocr_events", side_effect=fake_detect):
            _scan_source_worker(Path("video.mp4"), "ffprobe.exe", "en", [], OcrConfig())
        _WORKER_OCR_BACKENDS.clear()

        self.assertIsNone(captured["frame_source_factory"])

    def test_run_uses_ffmpeg_frame_source_factory_for_sequential_scan(self):
        from pubg_highlight_trim.frame_source import FfmpegFrameSource

        captured = {}

        def fake_detect(*_args, **kwargs):
            captured.update(kwargs)
            return [EventDetection(60.0, None, "no-event", "ocr")]

        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "source.mp4"
            source.touch()
            args = build_parser().parse_args([str(source), "--game-lang", "en", "-y", "--jobs", "1"])
            with patch(
                "pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=("ffmpeg-path", "ffprobe-path")
            ), patch("pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())), patch(
                "pubg_highlight_trim.pipeline.duration_sec", return_value=60.0
            ), patch("pubg_highlight_trim.pipeline.detect_ocr_events", side_effect=fake_detect), redirect_stdout(
                StringIO()
            ):
                run(args)

        factory = captured["frame_source_factory"]
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n") as probe:
            frame_source = factory(Path("video.mp4"))

        self.assertIsInstance(frame_source, FfmpegFrameSource)
        self.assertEqual(probe.call_args[0][0][0], "ffprobe-path")

    def test_automatic_jobs_is_one_for_single_source(self):
        self.assertEqual(_automatic_jobs(1, cpu_count=16, available_memory_gb=32.0), 1)

    def test_automatic_jobs_is_conservative_on_small_systems(self):
        self.assertEqual(_automatic_jobs(10, cpu_count=4, available_memory_gb=32.0), 1)
        self.assertEqual(_automatic_jobs(10, cpu_count=16, available_memory_gb=8.0), 1)

    def test_automatic_jobs_uses_two_for_capable_multi_file_system(self):
        self.assertEqual(_automatic_jobs(10, cpu_count=16, available_memory_gb=32.0), 2)

    def test_explicit_jobs_overrides_automatic_selection(self):
        self.assertEqual(_effective_jobs(1, 10), (1, False))
        self.assertEqual(_effective_jobs(3, 10), (3, False))

    def test_multiple_explicit_files_are_processed_in_recording_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            second = root / "PLAYERUNKNOWN'S BATTLEGROUNDS 2026.08.30 - 22.00.00.65.淘汰.DVR.mp4"
            first = root / "PLAYERUNKNOWN'S BATTLEGROUNDS 2026.08.30 - 09.00.00.65.淘汰.DVR.mp4"
            second.touch()
            first.touch()
            args = SimpleNamespace(input=None, files=[second, first], game_lang="en")

            input_paths, directory_input, single_file, base_folder = _validate_inputs(args)
            files, _ = _select_input_files(input_paths, args, directory_input)

        self.assertEqual(files, [first.resolve(), second.resolve()])
        self.assertFalse(directory_input)
        self.assertFalse(single_file)
        self.assertEqual(base_folder, root.resolve())

    def test_multiple_inputs_reject_directories(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            video = root / "video.mp4"
            video.touch()

            with self.assertRaisesRegex(SystemExit, "must all be mp4 files"):
                _validate_inputs(SimpleNamespace(input=None, files=[video, root]))

    def test_multiple_inputs_reject_non_mp4_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            video = root / "video.mp4"
            text = root / "notes.txt"
            video.touch()
            text.touch()

            with self.assertRaisesRegex(SystemExit, "must be .mp4 files"):
                _validate_inputs(SimpleNamespace(input=None, files=[video, text]))

    def test_rejects_positional_input_with_files_option(self):
        with self.assertRaisesRegex(SystemExit, "either the positional input or --files"):
            _validate_inputs(SimpleNamespace(input=Path("folder"), files=[Path("video.mp4")]))

    def test_merge_output_must_not_match_an_input_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first = root / "first.mp4"
            second = root / "second.mp4"
            first.touch()
            second.touch()
            args = SimpleNamespace(output_dir=None, merge=str(second), overwrite=True)

            with self.assertRaisesRegex(SystemExit, "must not overwrite an input file"):
                _prepare_output_paths(args, first, [first, second], root, single_file=False)

    def test_default_merge_output_is_inside_folder_output_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first = root / "first.mp4"
            first.touch()
            args = SimpleNamespace(output_dir=None, merge=True, overwrite=True)

            outdir, merged = _prepare_output_paths(args, first, [first], root, single_file=False)

            self.assertEqual(outdir, root / "pubg_highlight_trim_output")
            self.assertEqual(merged, outdir / "pubg_highlight_trim_merged.mp4")

    def test_default_merge_output_follows_unique_output_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first = root / "first.mp4"
            first.touch()
            (root / "pubg_highlight_trim_output").mkdir()
            args = SimpleNamespace(output_dir=None, merge=True, overwrite=False)

            outdir, merged = _prepare_output_paths(args, first, [first], root, single_file=False)

            self.assertEqual(outdir, root / "pubg_highlight_trim_output_1")
            self.assertEqual(merged, outdir / "pubg_highlight_trim_merged.mp4")

    def test_default_single_file_merge_output_is_inside_clip_output_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first = root / "match.mp4"
            first.touch()
            args = SimpleNamespace(output_dir=None, merge=True, overwrite=True)

            outdir, merged = _prepare_output_paths(args, first, [first], root, single_file=True)

            self.assertEqual(outdir, root / "match_pubg_trim_clips")
            self.assertEqual(merged, outdir / "match_pubg_trim.mp4")

    def test_merges_overlapping_events_into_one_clip(self):
        detections = [
            EventDetection(60.0, 30.5, "paddle-own-kill-text", "ocr", target="own-kill", event_key="own-kill:knock:a"),
            EventDetection(60.0, 32.0, "paddle-own-kill-text", "ocr", target="own-kill", event_key="own-kill:knock:b"),
        ]

        plans = _merged_clip_plans(detections, before=4.0, after=1.0)

        self.assertEqual(len(plans), 1)
        detection, start, end = plans[0]
        self.assertEqual(detection.event_count, "2")
        self.assertEqual(detection.event_secs, "30.500;32.000")
        self.assertEqual(detection.event_key, "own-kill:knock:a;own-kill:knock:b")
        self.assertAlmostEqual(start, 26.5)
        self.assertAlmostEqual(end, 33.0)

    def test_molotov_elim_uses_event_specific_before(self):
        class Args:
            seconds_before = 5.0
            seconds_after = 1.0
            molotov_elim_before = 10.0

        detections = [
            _apply_context_rules(
                EventDetection(
                    90.0,
                    73.5,
                    "paddle-own-kill-text",
                    "ocr",
                    target="own-kill",
                    event_key="own-kill:eliminate:enemya",
                    event_weapon="燃烧弹",
                ),
                Args(),
            )
        ]

        plans = _merged_clip_plans(detections, before=5.0, after=1.0)

        self.assertEqual(len(plans), 1)
        detection, start, end = plans[0]
        self.assertEqual(detection.context_rule, "molotov-elim-context")
        self.assertAlmostEqual(detection.clip_before_sec, 10.0)
        self.assertAlmostEqual(start, 63.5)
        self.assertAlmostEqual(end, 74.5)

    def test_molotov_knock_keeps_default_before(self):
        class Args:
            seconds_before = 5.0
            seconds_after = 1.0
            molotov_elim_before = 10.0

        detection = _apply_context_rules(
            EventDetection(
                90.0,
                67.0,
                "paddle-own-kill-text",
                "ocr",
                target="own-kill",
                event_key="own-kill:knock:enemya",
                event_weapon="燃烧弹",
            ),
            Args(),
        )

        self.assertEqual(detection.context_rule, "default")
        self.assertAlmostEqual(detection.clip_before_sec, 5.0)

    def test_record_detection_writes_context_columns(self):
        detection = EventDetection(
            90.0,
            73.5,
            "paddle-own-kill-text",
            "ocr",
            target="own-kill",
            event_key="own-kill:eliminate:enemya",
            event_secs="73.500",
            event_weapon="燃烧弹",
            context_rule="molotov-elim-context",
            clip_before_sec=10.0,
            clip_after_sec=1.0,
        )

        row = _record_detection(1, Path("clip.mp4"), detection, 63.5, 74.5, "", "", 0.0, 0.0, 0.0)

        self.assertEqual(row["EventWeapon"], "燃烧弹")
        self.assertEqual(row["ContextRule"], "molotov-elim-context")
        self.assertEqual(row["BeforeSec"], "10.000")
        self.assertEqual(row["AfterSec"], "1.000")

    def test_partitions_events_before_min_event_sec(self):
        detections = [
            EventDetection(60.0, 0.0, "paddle-own-kill-text", "ocr", target="own-kill", event_key="own-kill:knock:a"),
            EventDetection(60.0, 30.5, "paddle-own-kill-text", "ocr", target="own-kill", event_key="own-kill:knock:b"),
        ]

        kept, skipped = _partition_too_early_detections(detections, min_event_sec=2.0)

        self.assertEqual([d.event_sec for d in kept], [30.5])
        self.assertEqual([d.event_sec for d in skipped], [0.0])
        self.assertEqual(skipped[0].method, "skipped-before-min-event-sec")

    def test_skip_record_keeps_event_sec_for_auditing(self):
        detection = EventDetection(
            60.0,
            1.5,
            "skipped-before-min-event-sec",
            "ocr",
            target="own-kill",
            event_key="own-kill:knock:a",
            event_secs="1.500",
        )

        row = _record_skip(1, Path("clip.mp4"), detection)

        self.assertEqual(row["EventSec"], "1.500")
        self.assertEqual(row["EventSecs"], "1.500")
        self.assertEqual(row["EventCount"], "1")

    def test_scan_mode_auto_uses_fast_only_when_candidates_exist(self):
        import tempfile

        args = SimpleNamespace(scan_mode="auto")
        with tempfile.TemporaryDirectory() as tmp:
            candidate = Path(tmp) / "candidate_events.csv"
            candidate.write_text("Name,EventSec\n", encoding="utf-8")
            self.assertTrue(_use_fast_scan(args, candidate))
        self.assertFalse(_use_fast_scan(args, Path("missing_candidate_events.csv")))
        self.assertFalse(_use_fast_scan(args, None))

    def test_scan_mode_explicit_flags_override_auto(self):
        self.assertTrue(_use_fast_scan(SimpleNamespace(scan_mode="fast"), None))
        self.assertFalse(_use_fast_scan(SimpleNamespace(scan_mode="full"), Path("candidate_events.csv")))

    def test_multi_kill_sources_force_full_scan_even_in_fast_mode(self):
        profile = get_game_language_profile("en")
        config = OcrConfig(no_full_scan=True, language=profile)

        effective = _ocr_config_for_source(config, Path("a.Multi kill.DVR.mp4"), profile)

        self.assertFalse(effective.no_full_scan)

        hant = get_game_language_profile("zh-Hant")
        hant_effective = _ocr_config_for_source(
            OcrConfig(no_full_scan=True, language=hant),
            Path("a.多重擊殺.DVR.mp4"),
            hant,
        )

        self.assertFalse(hant_effective.no_full_scan)

    def test_match_end_sources_disable_brightness_gate(self):
        profile = get_game_language_profile("en")
        config = OcrConfig(no_full_scan=False, brightness_gate=True)

        source_config = _ocr_config_for_source(config, Path("a.End of match.DVR.mp4"), profile)

        self.assertFalse(source_config.brightness_gate)
        self.assertTrue(config.brightness_gate)

    def test_match_end_sources_force_full_scan_even_in_fast_mode(self):
        profile = get_game_language_profile("en")
        config = OcrConfig(no_full_scan=True, language=profile)

        effective = _ocr_config_for_source(config, Path("a.End of match.DVR.mp4"), profile)

        self.assertFalse(effective.no_full_scan)

    def test_single_kill_sources_keep_fast_scan(self):
        profile = get_game_language_profile("en")
        config = OcrConfig(no_full_scan=True, language=profile)

        effective = _ocr_config_for_source(config, Path("a.Single kill.DVR.mp4"), profile)

        self.assertTrue(effective.no_full_scan)

    def test_single_file_defaults_to_no_merge(self):
        self.assertTrue(_effective_no_merge(SimpleNamespace(merge=None), single_file=True))
        self.assertFalse(_effective_no_merge(SimpleNamespace(merge=None), single_file=False))
        self.assertFalse(_effective_no_merge(SimpleNamespace(merge=True), single_file=True))
        self.assertFalse(_effective_no_merge(SimpleNamespace(merge="merged.mp4"), single_file=True))
        self.assertTrue(_effective_no_merge(SimpleNamespace(merge=False), single_file=False))

    def test_merge_output_override_uses_path_value_only(self):
        self.assertEqual(_merge_output_override(SimpleNamespace(merge="merged.mp4")), Path("merged.mp4"))
        self.assertIsNone(_merge_output_override(SimpleNamespace(merge=True)))
        self.assertIsNone(_merge_output_override(SimpleNamespace(merge=False)))
        self.assertIsNone(_merge_output_override(SimpleNamespace(merge=None)))

    def test_auto_discovers_latest_candidate_csv(self):
        import os
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            old = root / "fullscan_old" / "candidate_events.csv"
            new = root / "fullscan_new" / "candidate_events.csv"
            old.parent.mkdir()
            new.parent.mkdir()
            old.write_text("Name,EventSec\n", encoding="utf-8")
            new.write_text("Name,EventSec\n", encoding="utf-8")
            os.utime(old, (1_700_000_000, 1_700_000_000))
            os.utime(new, (1_800_000_000, 1_800_000_000))

            path = _candidate_csv_path(SimpleNamespace(candidate_csv=None, no_auto_candidate_csv=False), root, root)

        self.assertEqual(path, new)

    def test_move_originals_archives_included_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first = root / "first.mp4"
            second = root / "second.mp4"
            output_dir = root / "output"
            first.touch()
            second.touch()
            args = build_parser().parse_args(
                [
                    "--files",
                    str(first),
                    str(second),
                    "--game-lang",
                    "en",
                    "--output-dir",
                    str(output_dir),
                    "-y",
                    "--jobs",
                    "1",
                    "--move-originals",
                ]
            )

            included = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            skipped = EventDetection(60.0, None, "paddle-not-target-text", "ocr", target="own-kill")
            code, _t, _c = self._run_with_mocked_video_pipeline_multi(args, [included, skipped])

            self.assertEqual(code, 0)
            archived = output_dir / "originals" / "first.mp4"
            self.assertTrue(archived.is_file())
            self.assertFalse(first.exists())
            self.assertTrue(second.is_file(), "skipped source must stay in place")
            manifest = output_dir / "moved_sources.csv"
            rows = list(csv.DictReader(manifest.open(encoding="utf-8-sig")))
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]["Name"], "first.mp4")
            # Temp paths may surface as 8.3 names (e.g. RUNNER~1) on CI; compare resolved.
            self.assertEqual(Path(rows[0]["SourcePath"]).resolve(), first.resolve())
            self.assertEqual(Path(rows[0]["ArchivePath"]).resolve(), archived.resolve())
            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["originals_moved"], 1)
            self.assertEqual(summary["originals_failed"], 0)

    def test_move_originals_dry_run_keeps_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first = root / "first.mp4"
            output_dir = root / "output"
            first.touch()
            args = build_parser().parse_args(
                [
                    "--files",
                    str(first),
                    "--game-lang",
                    "en",
                    "--output-dir",
                    str(output_dir),
                    "-y",
                    "--jobs",
                    "1",
                    "--move-originals",
                    "--dry-run",
                ]
            )

            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            code, _t, _c = self._run_with_mocked_video_pipeline(args, detection)

            self.assertEqual(code, 0)
            self.assertTrue(first.is_file())
            self.assertFalse((output_dir / "originals").exists())
            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["originals_moved"], 0)

    def _run_with_mocked_video_pipeline_multi(self, args, detections):
        def write_trim(_src, out, _start, _length, _ffmpeg):
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(b"trim")
            return "mock-encoder"

        def write_merge(clips, final, _ffmpeg):
            final.parent.mkdir(parents=True, exist_ok=True)
            final.write_bytes(b"merged")
            list_path = final.with_suffix(".concat.txt")
            list_path.write_text("\n".join(str(clip) for clip in clips), encoding="utf-8")
            return list_path

        queue = iter(detections)
        with patch("pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))), patch(
            "pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())
        ), patch("pubg_highlight_trim.pipeline.duration_sec", return_value=60.0), patch(
            "pubg_highlight_trim.pipeline.detect_ocr_events", side_effect=lambda *a, **k: [next(queue)]
        ), patch("pubg_highlight_trim.pipeline.trim_clip", side_effect=write_trim), patch(
            "pubg_highlight_trim.pipeline.concat_clips", side_effect=write_merge
        ):
            code = run(args)
        return code, None, None


if __name__ == "__main__":
    unittest.main()
