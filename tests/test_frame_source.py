import unittest

from pubg_highlight_trim.frame_source import Cv2FrameSource, VideoUnreadable
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

    def test_is_open_reflects_failed_open(self):
        class DeadCap:
            def isOpened(self):
                return False

            def release(self):
                pass

        class FakeCv2:
            CAP_PROP_POS_MSEC = 0

            def VideoCapture(self, _path):
                return DeadCap()

        self.assertFalse(Cv2FrameSource("clip.mp4", FakeCv2()).is_open)

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


if __name__ == "__main__":
    unittest.main()
