import statistics
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np

from pubg_highlight_trim.ffmpeg_tools import find_binary
from pubg_highlight_trim.frame_source import Cv2FrameSource, FfmpegFrameSource, VideoUnreadable
from pubg_highlight_trim.ocr import OcrConfig, detect_events


class Cv2FrameSourceTests(unittest.TestCase):
    def test_read_at_seeks_then_reads_through_capture(self):
        seeks = []

        class FakeCap:
            def __init__(self):
                self.frame = object()
                self.released = False

            def set(self, prop, value):
                seeks.append((prop, value))

            def read(self):
                return True, self.frame

            def isOpened(self):
                return True

            def release(self):
                self.released = True

        class FakeCv2:
            CAP_PROP_POS_MSEC = 7

            def __init__(self, cap):
                self.cap = cap
                self.paths = []

            def VideoCapture(self, path):
                self.paths.append(path)
                return self.cap

        cap = FakeCap()
        cv2 = FakeCv2(cap)
        source = Cv2FrameSource("clip.mp4", cv2)

        self.assertEqual(cv2.paths, ["clip.mp4"])
        self.assertTrue(source.is_open)
        self.assertIs(source.read_at(12.345), cap.frame)
        self.assertEqual(seeks, [(7, 12345.0)])
        source.close()
        self.assertTrue(cap.released)
        self.assertFalse(source.is_open)
        source.close()

    def test_read_at_returns_none_when_read_fails(self):
        class FailingCap:
            def set(self, *_args):
                pass

            def read(self):
                return False, None

            def isOpened(self):
                return True

            def release(self):
                pass

        class FakeCv2:
            CAP_PROP_POS_MSEC = 0

            def VideoCapture(self, _path):
                return FailingCap()

        source = Cv2FrameSource("clip.mp4", FakeCv2())

        self.assertTrue(source.is_open)
        self.assertIsNone(source.read_at(1.0))

    def test_failed_open_raises_video_unreadable(self):
        released = []

        class DeadCap:
            def isOpened(self):
                return False

            def release(self):
                released.append(True)

        class FakeCv2:
            CAP_PROP_POS_MSEC = 0

            def VideoCapture(self, _path):
                return DeadCap()

        with self.assertRaises(VideoUnreadable):
            Cv2FrameSource("clip.mp4", FakeCv2())
        self.assertEqual(released, [True])

    def test_cap_without_isopened_is_treated_as_open(self):
        class FakeCap:
            def set(self, *_args):
                pass

            def read(self):
                return False, None

        class FakeCv2:
            CAP_PROP_POS_MSEC = 0

            def VideoCapture(self, _path):
                return FakeCap()

        source = Cv2FrameSource("clip.mp4", FakeCv2())

        self.assertTrue(source.is_open)
        self.assertIsNone(source.read_at(1.0))

    def test_video_unreadable_is_runtime_error(self):
        self.assertTrue(issubclass(VideoUnreadable, RuntimeError))


class DetectEventsFrameSourceTests(unittest.TestCase):
    def test_detect_events_uses_factory_and_closes_source(self):
        used = []

        class FakeSource:
            is_open = True

            def read_at(self, _sec):
                return None

            def close(self):
                used.append("closed")

        def factory(path):
            used.append(path)
            return FakeSource()

        config = OcrConfig(no_full_scan=True, priority_window=[(1.0, 2.0)], candidate_step=1.0)
        detections = detect_events("clip.mp4", None, None, 4.0, [], config, frame_source_factory=factory)

        self.assertEqual(used, ["clip.mp4", "closed"])
        self.assertIsNone(detections[0].event_sec)
        self.assertEqual(detections[0].method, "paddle-no-target-text-found")


def _completed(stdout: bytes = b"", rc: int = 0, stderr: bytes = b"") -> subprocess.CompletedProcess:
    return subprocess.CompletedProcess([], rc, stdout=stdout, stderr=stderr)


class FfmpegFrameSourceTests(unittest.TestCase):
    def test_probe_parses_dimensions_and_reports_open(self):
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="1920,1080\n") as probe:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")

        argv = probe.call_args[0][0]
        self.assertEqual(argv[0], "ffprobe.exe")
        for token in ("-select_streams", "v:0", "-show_entries", "stream=width,height", "csv=p=0", "clip.mp4"):
            self.assertIn(token, argv)
        self.assertTrue(source.is_open)
        self.assertEqual((source.width, source.height), (1920, 1080))

    def test_probe_failure_raises_video_unreadable(self):
        with patch(
            "pubg_highlight_trim.frame_source.subprocess.check_output",
            side_effect=subprocess.CalledProcessError(1, [], stderr="moov atom not found"),
        ):
            with self.assertRaises(VideoUnreadable):
                FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")

    def test_probe_garbage_output_raises_video_unreadable(self):
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="garbage"):
            with self.assertRaises(VideoUnreadable):
                FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")

    def test_read_at_runs_ffmpeg_and_reshapes_bgr24(self):
        pixels = bytes(range(18))
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="3,2\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", return_value=_completed(pixels)
        ) as run:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")
            frame = source.read_at(1.5)

        argv = run.call_args[0][0]
        self.assertEqual(argv[0], "ffmpeg.exe")
        self.assertEqual(argv[argv.index("-ss") + 1], "1.500")
        self.assertEqual(argv[argv.index("-i") + 1], "clip.mp4")
        self.assertEqual(argv[argv.index("-frames:v") + 1], "1")
        self.assertEqual(argv[argv.index("-f") + 1], "rawvideo")
        self.assertEqual(argv[argv.index("-pix_fmt") + 1], "bgr24")
        self.assertEqual(argv[-1], "-")
        self.assertNotIn("-hwaccel", argv)
        self.assertEqual(run.call_args[1]["stderr"], subprocess.PIPE)
        self.assertEqual(frame.shape, (2, 3, 3))
        self.assertEqual(frame.dtype, np.uint8)
        self.assertEqual(frame.reshape(-1).tolist(), list(range(18)))

    def test_read_at_clamps_negative_seconds(self):
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", return_value=_completed(bytes(18))
        ) as run:
            FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe").read_at(-4.2)

        argv = run.call_args[0][0]
        self.assertEqual(argv[argv.index("-ss") + 1], "0.000")

    def test_hwaccel_flag_is_threaded_into_decode_argv(self):
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", return_value=_completed(bytes(18))
        ) as run:
            FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe", hwaccel="auto").read_at(0.0)

        argv = run.call_args[0][0]
        self.assertEqual(argv[argv.index("-hwaccel") + 1], "auto")

    def test_hwaccel_failure_falls_back_to_software_and_disables_it(self):
        good = _completed(bytes(18))
        bad = _completed(rc=1, stderr=b"no device available")
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", side_effect=[bad, good, good]
        ) as run:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe", hwaccel="cuda")
            self.assertIsNotNone(source.read_at(1.0))
            self.assertIsNotNone(source.read_at(2.0))

        argv = [call[0][0] for call in run.call_args_list]
        self.assertIn("-hwaccel", argv[0])
        self.assertNotIn("-hwaccel", argv[1])
        self.assertNotIn("-hwaccel", argv[2])
        self.assertIsNone(source._hwaccel)

    def test_hwaccel_disabled_after_first_hardware_failure(self):
        # A failed hwaccel read is retried in software once, then hwaccel is
        # off for good — even when that software retry also fails.
        good = _completed(bytes(18))
        bad = _completed(rc=1, stderr=b"past eof")
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", side_effect=[bad, bad, good, good]
        ) as run:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe", hwaccel="cuda")
            self.assertIsNone(source.read_at(999.0))
            self.assertIsNotNone(source.read_at(1.0))

        argv = [call[0][0] for call in run.call_args_list]
        self.assertIn("-hwaccel", argv[0])
        for later in argv[1:]:
            self.assertNotIn("-hwaccel", later)

    def test_undecodable_file_with_hwaccel_still_raises_video_unreadable(self):
        bad = _completed(rc=1, stderr=b"broken")
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", side_effect=[bad, bad, bad]
        ) as run:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe", hwaccel="cuda")
            with self.assertRaises(VideoUnreadable):
                source.read_at(1.0)

        # hwaccel + software at the requested ts, then software-only at 0.0.
        self.assertEqual(run.call_count, 3)

    def test_decode_failure_returns_none_once_a_frame_decoded(self):
        good = _completed(bytes(18))
        bad = _completed(rc=1, stderr=b"boom")
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", side_effect=[good, bad]
        ) as run:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")
            self.assertIsNotNone(source.read_at(1.0))
            self.assertIsNone(source.read_at(2.0))

        self.assertEqual(run.call_count, 2)

    def test_undecodable_file_raises_video_unreadable_on_first_read(self):
        bad = _completed(rc=1, stderr=b"could not find codec")
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", side_effect=[bad, bad]
        ) as run:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")
            with self.assertRaises(VideoUnreadable) as ctx:
                source.read_at(1.0)

        self.assertEqual(run.call_count, 2)
        self.assertIn("could not find codec", str(ctx.exception))

    def test_first_read_failure_falls_back_to_zero_before_deciding(self):
        good = _completed(bytes(18))
        bad = _completed(rc=1, stderr=b"past eof")
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run", side_effect=[bad, good]
        ):
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")
            self.assertIsNone(source.read_at(999.0))

    def test_close_marks_source_not_open(self):
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"):
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")
        source.close()
        self.assertFalse(source.is_open)

    def test_read_at_returns_none_after_close(self):
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run"
        ) as run:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")
            source.close()
            self.assertIsNone(source.read_at(1.0))

        run.assert_not_called()

    def test_decode_timeout_is_a_decode_failure(self):
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run",
            side_effect=subprocess.TimeoutExpired(["ffmpeg.exe"], 60.0),
        ) as run:
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")
            with self.assertRaises(VideoUnreadable) as ctx:
                source.read_at(1.0)

        # First-read rules: requested ts plus the 0.0 fallback probe, then raise.
        self.assertEqual(run.call_count, 2)
        self.assertIn("timed out", str(ctx.exception))

    def test_decode_timeout_after_successful_frame_returns_none(self):
        good = _completed(bytes(18))
        with patch("pubg_highlight_trim.frame_source.subprocess.check_output", return_value="2,3\n"), patch(
            "pubg_highlight_trim.frame_source.subprocess.run",
            side_effect=[good, subprocess.TimeoutExpired(["ffmpeg.exe"], 60.0)],
        ):
            source = FfmpegFrameSource(Path("clip.mp4"), "ffmpeg.exe", "ffprobe.exe")
            self.assertIsNotNone(source.read_at(1.0))
            self.assertIsNone(source.read_at(2.0))


class FfmpegFrameSourceIntegrationTests(unittest.TestCase):
    def setUp(self):
        try:
            self.ffmpeg = find_binary("ffmpeg.exe")
            self.ffprobe = find_binary("ffprobe.exe")
        except FileNotFoundError as exc:
            self.skipTest(f"vendored ffmpeg/ffprobe unavailable: {exc}")
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)

    def _synthesize(self, name: str, *codec_args: str) -> Path:
        clip = Path(self._tmp.name) / name
        subprocess.run(
            [
                self.ffmpeg,
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=30",
                "-pix_fmt",
                "yuv420p",
                *codec_args,
                str(clip),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        return clip

    def _assert_decodes(self, clip: Path) -> None:
        with FfmpegFrameSource(clip, self.ffmpeg, self.ffprobe) as source:
            self.assertTrue(source.is_open)
            self.assertEqual((source.width, source.height), (320, 240))
            frame = source.read_at(1.0)
            self.assertIsNotNone(frame)
            self.assertEqual(frame.shape, (240, 320, 3))
            self.assertEqual(frame.dtype, np.uint8)
            self.assertGreater(int(frame.std()), 0)
            later = source.read_at(1.5)
            self.assertIsNotNone(later)
            self.assertFalse(np.array_equal(frame, later))
        self.assertFalse(source.is_open)

    def test_decodes_bgr_frames_from_h264_clip(self):
        self._assert_decodes(self._synthesize("clip.mp4", "-c:v", "libx264"))

    def test_decodes_bgr_frames_from_av1_clip(self):
        try:
            clip = self._synthesize("clip_av1.mp4", "-c:v", "libaom-av1", "-cpu-used", "8", "-crf", "40")
        except subprocess.CalledProcessError:
            self.skipTest("libaom-av1 encoder unavailable")
        self._assert_decodes(clip)

    def test_garbage_file_raises_video_unreadable(self):
        bad = Path(self._tmp.name) / "bad.mp4"
        bad.write_bytes(b"not a video")
        with self.assertRaises(VideoUnreadable):
            FfmpegFrameSource(bad, self.ffmpeg, self.ffprobe)

    def test_unavailable_hwaccel_falls_back_to_software_decode(self):
        clip = self._synthesize("clip_hwfallback.mp4", "-c:v", "libx264")
        with FfmpegFrameSource(clip, self.ffmpeg, self.ffprobe, hwaccel="not-a-real-hwaccel") as source:
            frame = source.read_at(1.0)
            self.assertIsNotNone(frame)
            self.assertEqual(frame.shape, (240, 320, 3))
            self.assertIsNone(source._hwaccel)
            self.assertIsNotNone(source.read_at(1.5))

    def test_av1_read_at_latency_within_h264_bound(self):
        try:
            av1 = self._synthesize("clip_av1_perf.mp4", "-c:v", "libaom-av1", "-cpu-used", "8", "-crf", "40")
        except subprocess.CalledProcessError:
            self.skipTest("libaom-av1 encoder unavailable")
        h264 = self._synthesize("clip_h264_perf.mp4", "-c:v", "libx264")

        def median_read_ms(clip: Path) -> float:
            latencies = []
            with FfmpegFrameSource(clip, self.ffmpeg, self.ffprobe) as source:
                for sec in (0.2, 0.8, 1.4, 0.5, 1.0):
                    started = time.perf_counter()
                    self.assertIsNotNone(source.read_at(sec))
                    latencies.append((time.perf_counter() - started) * 1000)
            return statistics.median(latencies)

        av1_ms = median_read_ms(av1)
        h264_ms = median_read_ms(h264)
        # Smoke bound, not the real benchmark (scripts/bench_frame_read.py):
        # catches a gross AV1 regression while staying flake-safe on tiny
        # synthesized clips where process spawn dominates.
        self.assertLessEqual(av1_ms, h264_ms * 4.0)


if __name__ == "__main__":
    unittest.main()
