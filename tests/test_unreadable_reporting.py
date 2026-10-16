import csv
import json
import subprocess
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

from pubg_highlight_trim.cli import build_parser
from pubg_highlight_trim.frame_source import VideoUnreadable
from pubg_highlight_trim.models import EventDetection
from pubg_highlight_trim.ocr import OcrConfig, detect_events
from pubg_highlight_trim.pipeline import _WORKER_OCR_BACKENDS, _scan_source_worker, run


class DetectEventsUnreadableTests(unittest.TestCase):
    def _config(self):
        return OcrConfig(no_full_scan=True, priority_window=[(1.0, 2.0)], candidate_step=1.0)

    def test_factory_video_unreadable_propagates(self):
        def factory(_path):
            raise VideoUnreadable("cannot decode")

        with self.assertRaises(VideoUnreadable):
            detect_events("clip.mp4", None, None, 4.0, [], self._config(), frame_source_factory=factory)

    def test_closed_factory_source_raises_and_is_closed(self):
        closed = []

        class DeadSource:
            is_open = False

            def read_at(self, _sec):
                return None

            def close(self):
                closed.append(True)

        with self.assertRaises(VideoUnreadable):
            detect_events(
                "clip.mp4",
                None,
                None,
                4.0,
                [],
                self._config(),
                frame_source_factory=lambda _path: DeadSource(),
            )
        self.assertEqual(closed, [True])

    def test_read_at_video_unreadable_propagates_and_closes(self):
        closed = []

        class DyingSource:
            is_open = True

            def read_at(self, _sec):
                raise VideoUnreadable("decoder died mid-stream")

            def close(self):
                closed.append(True)

        with self.assertRaises(VideoUnreadable):
            detect_events(
                "clip.mp4",
                None,
                None,
                4.0,
                [],
                self._config(),
                frame_source_factory=lambda _path: DyingSource(),
            )
        self.assertEqual(closed, [True])

    def test_failing_reads_alone_do_not_raise(self):
        class FailingSource:
            is_open = True

            def read_at(self, _sec):
                return None

            def close(self):
                pass

        detections = detect_events(
            "clip.mp4",
            None,
            None,
            4.0,
            [],
            self._config(),
            frame_source_factory=lambda _path: FailingSource(),
        )

        self.assertEqual(detections[0].method, "paddle-no-target-text-found")


class _FakeFuture:
    def __init__(self, behavior):
        self._behavior = behavior

    def result(self):
        kind, payload = self._behavior
        if kind == "err":
            raise payload
        return payload


def _fake_executor(behaviors):
    class FakeExecutor:
        def __init__(self, max_workers=None):
            self.max_workers = max_workers

        def __enter__(self):
            return self

        def __exit__(self, *_exc):
            return False

        def submit(self, _fn, src, *_args):
            return _FakeFuture(behaviors[src.name])

    return FakeExecutor


class UnreadableReportingTests(unittest.TestCase):
    def _parse_args(self, source: Path, output_dir: Path, *extra: str):
        return build_parser().parse_args(
            [
                str(source),
                "--game-lang",
                "en",
                "--output-dir",
                str(output_dir),
                "--dry-run",
                "-y",
                *extra,
            ]
        )

    def _run_sequential(self, source: Path, output_dir: Path, *extra: str):
        args = self._parse_args(source, output_dir, "--jobs", "1", *extra)
        stdout = StringIO()
        with patch("pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))), patch(
            "pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())
        ), patch("pubg_highlight_trim.pipeline.duration_sec", return_value=60.0), patch(
            "pubg_highlight_trim.pipeline.detect_ocr_events",
            side_effect=VideoUnreadable("cv2 could not open video source"),
        ), redirect_stdout(stdout):
            code = run(args)
        return code, stdout.getvalue()

    def test_sequential_unreadable_source_reports_skip_line(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "broken.mp4"
            source.touch()
            output_dir = root / "output"

            code, stdout = self._run_sequential(source, output_dir)

            self.assertEqual(code, 2)
            self.assertIn("[01/1] SKIP both video-unreadable lang=en | broken.mp4", stdout)
            self.assertNotIn("cv2 could not open", stdout)

            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["unreadable_count"], 1)
            # Unreadable files are not included, so they count as skipped too.
            self.assertEqual(summary["skipped_count"], 1)
            self.assertEqual(summary["included_count"], 0)
            self.assertEqual(summary["methods"].get("video-unreadable"), 1)
            self.assertEqual(summary["detectors"].get("ocr"), 1)

            with (output_dir / "检测与裁剪记录.csv").open(encoding="utf-8-sig", newline="") as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]["Status"], "unreadable")
            self.assertEqual(rows[0]["Method"], "video-unreadable")
            self.assertEqual(rows[0]["Detector"], "ocr")

    def test_verbose_skip_line_includes_unreadable_reason(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "broken.mp4"
            source.touch()
            output_dir = root / "output"

            code, stdout = self._run_sequential(source, output_dir, "--verbose")

            self.assertEqual(code, 2)
            self.assertIn(
                "[01/1] SKIP both video-unreadable lang=en | broken.mp4 | cv2 could not open video source",
                stdout,
            )

    def _run_parallel(self, behaviors, output_dir: Path, *sources: Path):
        args = build_parser().parse_args(
            [
                "--files",
                *[str(source) for source in sources],
                "--game-lang",
                "en",
                "--output-dir",
                str(output_dir),
                "--dry-run",
                "--jobs",
                "2",
                "-y",
            ]
        )
        stdout = StringIO()
        with patch("pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))), patch(
            "pubg_highlight_trim.pipeline.ProcessPoolExecutor", _fake_executor(behaviors)
        ), patch("pubg_highlight_trim.pipeline.as_completed", lambda futures: iter(futures)), redirect_stdout(stdout):
            code = run(args)
        return code, stdout.getvalue()

    def test_parallel_unreadable_worker_result_does_not_abort_batch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            good = root / "good.mp4"
            broken = root / "broken.mp4"
            good.touch()
            broken.touch()
            output_dir = root / "output"
            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            behaviors = {
                "good.mp4": ("ok", (60.0, 0.01, [detection], None)),
                # Error channel: the worker measured duration/probe before the
                # decode failure, so real timings reach the record.
                "broken.mp4": ("ok", (12.5, 0.25, None, "decoder init failed")),
            }
            code, output = self._run_parallel(behaviors, output_dir, broken, good)

            self.assertEqual(code, 0)
            self.assertIn("SKIP both video-unreadable lang=en | broken.mp4", output)
            self.assertIn("INCLUDE own-kill", output)

            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["unreadable_count"], 1)
            self.assertEqual(summary["skipped_count"], 1)
            self.assertEqual(summary["included_count"], 1)

            with (output_dir / "检测与裁剪记录.csv").open(encoding="utf-8-sig", newline="") as handle:
                rows = {row["Name"]: row for row in csv.DictReader(handle)}
            self.assertEqual(rows["broken.mp4"]["Status"], "unreadable")
            self.assertEqual(rows["broken.mp4"]["Method"], "video-unreadable")
            self.assertEqual(rows["broken.mp4"]["DurationSec"], "12.500")
            self.assertEqual(rows["broken.mp4"]["ProbeSeconds"], "0.250")
            self.assertEqual(rows["good.mp4"]["Status"], "included")

    def test_parallel_raised_video_unreadable_still_does_not_abort_batch(self):
        # Defensive: a future that raises VideoUnreadable outright (rather than
        # reporting it through the worker error channel) is still contained.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            good = root / "good.mp4"
            broken = root / "broken.mp4"
            good.touch()
            broken.touch()
            output_dir = root / "output"
            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            behaviors = {
                "good.mp4": ("ok", (60.0, 0.01, [detection], None)),
                "broken.mp4": ("err", VideoUnreadable("decoder init failed")),
            }
            code, output = self._run_parallel(behaviors, output_dir, broken, good)

            self.assertEqual(code, 0)
            self.assertIn("SKIP both video-unreadable lang=en | broken.mp4", output)

            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["unreadable_count"], 1)
            self.assertEqual(summary["included_count"], 1)

    def test_sequential_probe_failure_reports_unreadable(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "corrupt.mp4"
            source.touch()
            output_dir = root / "output"
            args = self._parse_args(source, output_dir, "--jobs", "1", "--verbose")
            stdout = StringIO()
            with patch(
                "pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))
            ), patch(
                "pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())
            ), patch(
                "pubg_highlight_trim.pipeline.duration_sec",
                side_effect=subprocess.CalledProcessError(1, ["ffprobe"], stderr="moov atom not found"),
            ), patch(
                "pubg_highlight_trim.pipeline.detect_ocr_events"
            ) as detect, redirect_stdout(stdout):
                code = run(args)
            output = stdout.getvalue()

            self.assertEqual(code, 2)
            self.assertIn("[01/1] SKIP both video-unreadable lang=en | corrupt.mp4", output)
            self.assertIn("ffprobe duration probe failed for corrupt.mp4", output)
            detect.assert_not_called()

            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["unreadable_count"], 1)
            self.assertEqual(summary["skipped_count"], 1)
            self.assertEqual(summary["included_count"], 0)
            self.assertEqual(summary["methods"].get("video-unreadable"), 1)

            with (output_dir / "检测与裁剪记录.csv").open(encoding="utf-8-sig", newline="") as handle:
                rows = list(csv.DictReader(handle))
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]["Status"], "unreadable")
            self.assertEqual(rows[0]["Method"], "video-unreadable")

    def test_sequential_probe_garbage_duration_reports_unreadable(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "corrupt.mp4"
            source.touch()
            output_dir = root / "output"
            args = self._parse_args(source, output_dir, "--jobs", "1")
            with patch(
                "pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))
            ), patch(
                "pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())
            ), patch(
                "pubg_highlight_trim.pipeline.duration_sec", side_effect=ValueError("could not convert 'N/A'")
            ), patch(
                "pubg_highlight_trim.pipeline.detect_ocr_events"
            ) as detect, redirect_stdout(StringIO()):
                code = run(args)

            self.assertEqual(code, 2)
            detect.assert_not_called()
            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["unreadable_count"], 1)

    def test_cache_hit_probe_failure_reports_unreadable(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "corrupt.mp4"
            source.touch()
            output_dir = root / "output"
            args = self._parse_args(source, output_dir, "--jobs", "1", "--cache-dir", str(root / "cache"))
            stdout = StringIO()
            with patch(
                "pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))
            ), patch(
                "pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())
            ), patch(
                "pubg_highlight_trim.pipeline.load_detection_cache", return_value=[]
            ), patch(
                "pubg_highlight_trim.pipeline.duration_sec",
                side_effect=subprocess.CalledProcessError(1, ["ffprobe"], stderr="moov atom not found"),
            ), patch(
                "pubg_highlight_trim.pipeline.detect_ocr_events"
            ) as detect, redirect_stdout(stdout):
                code = run(args)
            output = stdout.getvalue()

            self.assertEqual(code, 2)
            self.assertIn("[01/1] SKIP both video-unreadable lang=en | corrupt.mp4", output)
            detect.assert_not_called()

            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["unreadable_count"], 1)
            self.assertEqual(summary["skipped_count"], 1)
            self.assertEqual(summary["cache_hits"], 1)

    def test_parallel_probe_failure_reports_unreadable_with_reason(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            good = root / "good.mp4"
            broken = root / "broken.mp4"
            good.touch()
            broken.touch()
            output_dir = root / "output"
            detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")
            behaviors = {
                "good.mp4": ("ok", (60.0, 0.01, [detection], None)),
                "broken.mp4": (
                    "ok",
                    (0.0, 0.02, None, "ffprobe duration probe failed for broken.mp4: exit status 1"),
                ),
            }
            code, output = self._run_parallel(behaviors, output_dir, broken, good)

            self.assertEqual(code, 0)
            self.assertIn("SKIP both video-unreadable lang=en | broken.mp4", output)
            self.assertIn("INCLUDE own-kill", output)

            summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["unreadable_count"], 1)
            self.assertEqual(summary["skipped_count"], 1)
            self.assertEqual(summary["included_count"], 1)

    def test_scan_worker_reports_video_unreadable_via_error_channel(self):
        _WORKER_OCR_BACKENDS.clear()
        with patch("pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())), patch(
            "pubg_highlight_trim.pipeline.duration_sec", return_value=60.0
        ), patch(
            "pubg_highlight_trim.pipeline.detect_ocr_events",
            side_effect=VideoUnreadable("cannot decode"),
        ):
            duration, probe_seconds, detections, error = _scan_source_worker(
                Path("broken.mp4"), Path("ffprobe"), "en", [], OcrConfig()
            )
        _WORKER_OCR_BACKENDS.clear()

        self.assertEqual(duration, 60.0)
        self.assertGreaterEqual(probe_seconds, 0.0)
        self.assertIsNone(detections)
        self.assertEqual(error, "cannot decode")

    def test_scan_worker_reports_probe_failure_via_error_channel(self):
        _WORKER_OCR_BACKENDS.clear()
        with patch("pubg_highlight_trim.pipeline.load_backend", return_value=(object(), object())), patch(
            "pubg_highlight_trim.pipeline.duration_sec",
            side_effect=subprocess.CalledProcessError(1, ["ffprobe"], stderr="moov atom not found"),
        ), patch("pubg_highlight_trim.pipeline.detect_ocr_events") as detect:
            duration, probe_seconds, detections, error = _scan_source_worker(
                Path("corrupt.mp4"), Path("ffprobe"), "en", [], OcrConfig()
            )
        _WORKER_OCR_BACKENDS.clear()

        self.assertEqual(duration, 0.0)
        self.assertGreaterEqual(probe_seconds, 0.0)
        self.assertIsNone(detections)
        self.assertIn("ffprobe duration probe failed for corrupt.mp4", error)
        detect.assert_not_called()


if __name__ == "__main__":
    unittest.main()
