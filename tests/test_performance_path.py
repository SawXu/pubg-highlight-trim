import csv
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from pubg_highlight_trim.cache import cache_key, load_detection_cache, save_detection_cache
from pubg_highlight_trim.cli import build_parser
from pubg_highlight_trim.models import EventDetection
from pubg_highlight_trim.ocr import OcrConfig, build_adaptive_scan_times, ocr_assist_at, ocr_at
from pubg_highlight_trim.pipeline import CANDIDATE_CSV_COLUMNS, _candidate_row, _write_candidate_csv


class PerformancePathTests(unittest.TestCase):
    def test_fast_path_flags_and_budgets_are_configurable(self):
        args = build_parser().parse_args(
            [
                "video.mp4",
                "--fast-path",
                "--sampling-mode",
                "adaptive",
                "--gate-mode",
                "light",
                "--ocr-max-calls",
                "12",
                "--refine-max-frames",
                "4",
                "--assist-max-frames",
                "3",
            ]
        )
        self.assertTrue(args.fast_path)
        self.assertEqual(args.brightness_gate_mode, "light")
        self.assertEqual(args.ocr_max_calls, 12)
        self.assertEqual(args.refine_max_frames, 4)
        self.assertEqual(args.assist_max_frames, 3)

    def test_roi_and_budget_validation(self):
        with self.assertRaises(ValueError):
            OcrConfig(roi=(0.8, 0.1, 0.2, 0.3))
        with self.assertRaises(ValueError):
            OcrConfig(ocr_max_calls=-1)
        with self.assertRaises(ValueError):
            OcrConfig(scan_start=4.0, scan_end=4.0)

    def test_adaptive_schedule_is_sorted_and_deduplicated(self):
        config = OcrConfig(
            sampling_mode="adaptive",
            priority_window=[(3.0, 4.0)],
            candidate_lookback=1.0,
            candidate_lookahead=0.5,
            coarse_step=2.0,
            adaptive_step=0.5,
        )
        samples = build_adaptive_scan_times(8.0, [3.5], config)
        self.assertEqual(samples, sorted(set(samples)))
        self.assertIn(3.5, samples)

    def test_adaptive_schedule_keeps_candidate_anchor_without_dense_grid(self):
        config = OcrConfig(
            sampling_mode="adaptive",
            priority_window=[],
            candidate_lookback=1.0,
            candidate_lookahead=0.5,
            coarse_step=2.0,
            candidate_step=4.0,
            adaptive_step=0.5,
        )

        samples = build_adaptive_scan_times(8.0, [3.5], config)

        self.assertIn(3.5, samples)
        self.assertIn(2.5, samples)
        self.assertNotIn(3.0, samples)
        self.assertEqual(len(samples), len(set(samples)))

    def test_primary_and_assist_roi_share_one_decoded_frame(self):
        class Frame:
            shape = (10, 10, 3)

            def __getitem__(self, _key):
                return self

        class Capture:
            def __init__(self):
                self.read_count = 0

            def set(self, *_args):
                pass

            def read(self):
                self.read_count += 1
                return True, Frame()

        class Cv2:
            CAP_PROP_POS_MSEC = 0
            INTER_AREA = 0

        config = OcrConfig(ocr_width=0, brightness_gate=False)
        capture = Capture()
        with patch("pubg_highlight_trim.ocr._predict_text", return_value=("", "", 0.01)):
            ocr_at(Cv2(), capture, None, 10.0, config)
            ocr_assist_at(Cv2(), capture, None, 10.0, config)
        self.assertEqual(capture.read_count, 1)

    def test_cache_requires_completion_marker_and_invalidates_key(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "video.mp4"
            source.write_bytes(b"video-a")
            config = OcrConfig()
            key = cache_key(source, config)
            save_detection_cache(root / "cache", key, [EventDetection(12.0, None, "none", "ocr")])
            self.assertIsNotNone(load_detection_cache(root / "cache", key))
            (root / "cache" / f"{key}.complete").unlink()
            self.assertIsNone(load_detection_cache(root / "cache", key))
            source.write_bytes(b"video-b")
            self.assertNotEqual(key, cache_key(source, config))

    def test_candidate_csv_has_stable_columns_and_run_id(self):
        record = {
            "Name": "clip.mp4",
            "Status": "included",
            "Target": "own-kill",
            "EventSec": "10.000",
            "EventSecs": "10.000",
            "KeepStartSec": "5.000",
            "KeepEndSec": "11.000",
            "PaddleScores": "0.990",
            "OcrGateSkippedFrames": "2",
            "OcrGateReason": "light-low-brightness",
            "OcrCoarseFrames": "4",
            "OcrRefineFrames": "3",
            "Method": "paddle-own-kill-text",
            "Detector": "ocr",
        }
        row = _candidate_row("run-1", record, "hit")
        self.assertEqual(list(row), CANDIDATE_CSV_COLUMNS)
        self.assertEqual(row["RunId"], "run-1")
        self.assertEqual(row["GateStatus"], "skipped")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "candidate_events.csv"
            _write_candidate_csv(path, "run-1", [record], {"clip.mp4": "hit"})
            with path.open(encoding="utf-8-sig", newline="") as handle:
                parsed = list(csv.DictReader(handle))
            self.assertEqual(parsed[0]["CacheStatus"], "hit")
            self.assertEqual(parsed[0]["SchemaVersion"], "1")


if __name__ == "__main__":
    unittest.main()
