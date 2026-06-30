import unittest
from unittest.mock import patch

from pubg_highlight_trim.ocr import OcrConfig, OcrResult, refine_event


class OcrRefineTests(unittest.TestCase):
    def test_refine_walks_backward_from_coarse_hit(self):
        seen: list[float] = []

        def fake_ocr_at(_cv2, _cap, _ocr, sec, _config):
            seen.append(sec)
            if sec >= 30.5:
                return OcrResult("击倒了你", "", 0.01, "paddle-strict-self-text")
            return OcrResult("", "", 0.01, "paddle-not-self-text")

        config = OcrConfig(refine_before=3.0, refine_step=0.5)
        coarse = OcrResult("击倒了你", "", 0.0, "paddle-strict-self-text")
        with patch("pubg_highlight_trim.ocr.ocr_at", fake_ocr_at):
            event_sec, result, sampled, ocr_seconds, frame_seconds = refine_event(None, None, None, 31.0, 60.0, config, coarse)

        self.assertEqual(event_sec, 30.5)
        self.assertEqual(result.method, "paddle-strict-self-text")
        self.assertEqual(sampled, 2)
        self.assertAlmostEqual(ocr_seconds, 0.02)
        self.assertAlmostEqual(frame_seconds, 0.0)
        self.assertEqual(seen, [30.5, 30.0])


if __name__ == "__main__":
    unittest.main()
