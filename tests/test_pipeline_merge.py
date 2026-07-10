import unittest
import tempfile
from pathlib import Path
from types import SimpleNamespace

from pubg_highlight_trim.models import EventDetection
from pubg_highlight_trim.game_languages import get_game_language_profile
from pubg_highlight_trim.ocr import OcrConfig
from pubg_highlight_trim.pipeline import (
    _apply_context_rules,
    _candidate_csv_path,
    _effective_no_merge,
    _select_input_files,
    _validate_inputs,
    _merge_output_override,
    _merged_clip_plans,
    _ocr_config_for_source,
    _partition_too_early_detections,
    _prepare_output_paths,
    _record_detection,
    _record_skip,
    _use_fast_scan,
)


class PipelineMergeTests(unittest.TestCase):
    def test_multiple_explicit_files_preserve_command_line_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            second = root / "second.mp4"
            first = root / "first.mp4"
            second.touch()
            first.touch()
            args = SimpleNamespace(input=None, files=[second, first], game_lang="en")

            input_paths, directory_input, single_file, base_folder = _validate_inputs(args)
            files, _ = _select_input_files(input_paths, args, directory_input)

        self.assertEqual(files, [second.resolve(), first.resolve()])
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


if __name__ == "__main__":
    unittest.main()
