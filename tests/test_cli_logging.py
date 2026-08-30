import tempfile
import unittest
import sys
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from pubg_highlight_trim.cli import build_parser, main
from pubg_highlight_trim.models import EventDetection
from pubg_highlight_trim.pipeline import _scan_source_worker, run


class CliLoggingTests(unittest.TestCase):
    def _run(self, *options: str, detection: EventDetection | None = None):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "sample.mp4"
            source.touch()
            args = build_parser().parse_args([str(source), "--game-lang", "en", "-y", *options])
            stdout = StringIO()
            stderr = StringIO()
            found = detection if detection is not None else EventDetection(60.0, None, "no-event", "ocr")
            def load_backend(*_args, **_kwargs):
                print("third-party OCR initialization", flush=True)
                print("third-party OCR error", file=sys.stderr, flush=True)
                return object(), object()

            def detect_events(*_args, **_kwargs):
                print("third-party OCR frame diagnostics", flush=True)
                print("third-party OCR frame error", file=sys.stderr, flush=True)
                return [found]

            with patch("pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))), patch(
                "pubg_highlight_trim.pipeline.load_backend", side_effect=load_backend
            ), patch("pubg_highlight_trim.pipeline.duration_sec", return_value=60.0), patch(
                "pubg_highlight_trim.pipeline.detect_ocr_events", side_effect=detect_events
            ), redirect_stdout(stdout), redirect_stderr(stderr):
                code = run(args)
            return code, stdout.getvalue(), stderr.getvalue()

    def test_default_output_contains_only_application_records(self):
        code, stdout, stderr = self._run()

        self.assertEqual(code, 2)
        self.assertEqual(stderr, "")
        self.assertIn("PROGRESS ", stdout)
        self.assertIn("SKIP ", stdout)
        self.assertIn("SUMMARY ", stdout)
        self.assertNotIn("windows_only=", stdout)
        self.assertNotIn("third-party", stdout)
        self.assertNotIn("third-party", stderr)
        self.assertNotIn("PROFILE ", stdout)

    def test_profile_adds_timings_without_startup_or_third_party_diagnostics(self):
        code, stdout, stderr = self._run("--profile")

        self.assertEqual(code, 2)
        self.assertEqual(stderr, "")
        self.assertIn("PROFILE ", stdout)
        self.assertNotIn("windows_only=", stdout)
        self.assertNotIn("third-party", stdout)
        self.assertNotIn("third-party", stderr)
        self.assertNotIn("scan_mode=", stdout)
        self.assertNotIn("candidate_csv=", stdout)

    def test_verbose_adds_startup_diagnostics(self):
        code, stdout, stderr = self._run("--verbose")

        self.assertEqual(code, 2)
        self.assertIn("windows_only=true", stdout)
        self.assertIn("detector=ocr", stdout)
        self.assertIn("third-party OCR", stdout)
        self.assertIn("third-party OCR error", stderr)
        self.assertNotIn("PROFILE ", stdout)

    def test_included_run_preserves_success_exit_code_and_summary(self):
        detection = EventDetection(60.0, 30.0, "paddle-own-kill-text", "ocr", target="own-kill")

        code, stdout, stderr = self._run("--scan-only", detection=detection)

        self.assertEqual(code, 0)
        self.assertEqual(stderr, "")
        self.assertIn("INCLUDE ", stdout)
        self.assertIn('"included_count": 1', stdout)

    def test_ocr_error_is_actionable_without_traceback(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "sample.mp4"
            source.touch()
            args = build_parser().parse_args([str(source), "--game-lang", "en", "-y"])
            with patch("pubg_highlight_trim.pipeline.find_ffmpeg_pair", return_value=(Path("ffmpeg"), Path("ffprobe"))):
                with patch("pubg_highlight_trim.cli.platform.system", return_value="Windows"), patch(
                    "pubg_highlight_trim.cli.run", side_effect=RuntimeError("synthetic backend failure")
                ), redirect_stdout(StringIO()) as stdout, redirect_stderr(StringIO()) as stderr:
                    code = main([str(source), "--game-lang", "en", "-y"])

            self.assertEqual(code, 1)
            self.assertEqual(stdout.getvalue(), "")
            self.assertEqual(stderr.getvalue(), "ERROR synthetic backend failure\n")

    def test_verbose_error_includes_traceback(self):
        with patch("pubg_highlight_trim.cli.platform.system", return_value="Windows"), patch(
            "pubg_highlight_trim.cli.run", side_effect=RuntimeError("synthetic backend failure")
        ), redirect_stderr(StringIO()) as stderr:
            code = main(["video.mp4", "--verbose"])

        self.assertEqual(code, 1)
        self.assertIn("ERROR synthetic backend failure\n", stderr.getvalue())
        self.assertIn("Traceback (most recent call last)", stderr.getvalue())

    def test_input_error_uses_stable_error_summary(self):
        with patch("pubg_highlight_trim.cli.platform.system", return_value="Windows"), redirect_stdout(
            StringIO()
        ), redirect_stderr(StringIO()) as stderr:
            code = main(["missing.mp4"])

        self.assertEqual(code, 1)
        self.assertTrue(stderr.getvalue().startswith("ERROR Input path does not exist: "))
        self.assertTrue(stderr.getvalue().endswith("missing.mp4\n"))

    def test_frozen_worker_suppresses_python_stdout_and_stderr(self):
        stdout = StringIO()
        stderr = StringIO()

        def noisy_backend(*_args, **_kwargs):
            print("worker stdout")
            print("worker stderr", file=sys.stderr)
            return object(), object()

        def noisy_detector(*_args, **_kwargs):
            print("detector stdout")
            print("detector stderr", file=sys.stderr)
            return []

        with patch("pubg_highlight_trim.pipeline.sys.frozen", True, create=True), patch(
            "pubg_highlight_trim.pipeline.load_backend", side_effect=noisy_backend
        ), patch("pubg_highlight_trim.pipeline.detect_ocr_events", side_effect=noisy_detector), patch(
            "pubg_highlight_trim.pipeline.duration_sec", return_value=60.0
        ), redirect_stdout(stdout), redirect_stderr(stderr):
            _scan_source_worker(Path("sample.mp4"), Path("ffprobe"), "en", [], SimpleNamespace(), False)

        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(stderr.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
