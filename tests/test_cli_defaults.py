import unittest
from contextlib import redirect_stderr
from io import StringIO

from pubg_highlight_trim.cli import build_parser


class CliDefaultsTests(unittest.TestCase):
    def test_defaults_match_common_pubg_trim_workflow(self):
        args = build_parser().parse_args(["video.mp4"])

        self.assertEqual(args.detector, "ocr")
        self.assertEqual(args.target, "both")
        self.assertEqual(args.seconds_before, 5.0)
        self.assertEqual(args.seconds_after, 1.0)
        self.assertEqual(args.min_event_sec, 2.0)
        self.assertEqual(args.molotov_elim_before, 10.0)
        self.assertEqual(args.scan_mode, "auto")
        self.assertIsNone(args.merge)
        self.assertFalse(args.no_auto_candidate_csv)

    def test_scan_mode_aliases(self):
        parser = build_parser()

        self.assertEqual(parser.parse_args([".", "--full-scan"]).scan_mode, "full")
        self.assertEqual(parser.parse_args([".", "--fast-scan"]).scan_mode, "fast")
        self.assertEqual(parser.parse_args([".", "--no-full-scan"]).scan_mode, "fast")

    def test_merge_flags(self):
        parser = build_parser()

        self.assertTrue(parser.parse_args([".", "--merge"]).merge)
        self.assertEqual(parser.parse_args([".", "--merge", "merged.mp4"]).merge, "merged.mp4")
        self.assertFalse(parser.parse_args([".", "--no-merge"]).merge)

    def test_short_aliases_for_common_flags(self):
        args = build_parser().parse_args([".", "-o", "clip", "-y", "--scan-only"])

        self.assertEqual(str(args.output_dir), "clip")
        self.assertTrue(args.overwrite)
        self.assertTrue(args.dry_run)

    def test_final_and_montage_flags_are_removed(self):
        parser = build_parser()

        with redirect_stderr(StringIO()):
            with self.assertRaises(SystemExit):
                parser.parse_args([".", "--montage", "montage.mp4"])
            with self.assertRaises(SystemExit):
                parser.parse_args([".", "--final", "legacy.mp4"])
        self.assertNotIn("--montage", parser.format_help())
        self.assertNotIn("--final", parser.format_help())
        self.assertNotIn("FINAL", parser.format_help())


if __name__ == "__main__":
    unittest.main()
