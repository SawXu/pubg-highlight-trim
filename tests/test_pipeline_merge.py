import unittest
from pathlib import Path

from pubg_highlight_trim.models import EventDetection
from pubg_highlight_trim.pipeline import _merged_clip_plans, _partition_too_early_detections, _record_skip


class PipelineMergeTests(unittest.TestCase):
    def test_merges_overlapping_events_into_one_clip(self):
        detections = [
            EventDetection(60.0, 30.5, "paddle-own-kill-text", "ocr", target="own-kill", event_key="own-kill:击倒:a"),
            EventDetection(60.0, 32.0, "paddle-own-kill-text", "ocr", target="own-kill", event_key="own-kill:击倒:b"),
        ]

        plans = _merged_clip_plans(detections, before=4.0, after=1.0)

        self.assertEqual(len(plans), 1)
        detection, start, end = plans[0]
        self.assertEqual(detection.event_count, "2")
        self.assertEqual(detection.event_secs, "30.500;32.000")
        self.assertEqual(detection.event_key, "own-kill:击倒:a;own-kill:击倒:b")
        self.assertAlmostEqual(start, 26.5)
        self.assertAlmostEqual(end, 33.0)

    def test_partitions_events_before_min_event_sec(self):
        detections = [
            EventDetection(60.0, 0.0, "paddle-own-kill-text", "ocr", target="own-kill", event_key="own-kill:击倒:a"),
            EventDetection(60.0, 30.5, "paddle-own-kill-text", "ocr", target="own-kill", event_key="own-kill:击倒:b"),
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
            event_key="own-kill:击倒:a",
            event_secs="1.500",
        )

        row = _record_skip(1, Path("clip.mp4"), detection, opening_red_ratio=0.0)

        self.assertEqual(row["EventSec"], "1.500")
        self.assertEqual(row["EventSecs"], "1.500")
        self.assertEqual(row["EventCount"], "1")


if __name__ == "__main__":
    unittest.main()
