import unittest
from unittest.mock import patch

from pubg_highlight_trim.ocr import (
    OcrConfig,
    OcrResult,
    classify_own_kill_text,
    classify_target_kind,
    detect_assist_nearby,
    detect_events,
    extract_text_events,
    has_assist_text,
    is_assist_own_kill_text,
    is_delayed_own_elim_text,
    refine_event,
    same_noisy_close_event,
    same_text_event,
)


class OcrRefineTests(unittest.TestCase):
    def test_classifies_own_kill_text(self):
        self.assertEqual(classify_own_kill_text("你用M416淘汰了Player123"), "paddle-own-kill-text")
        self.assertIsNone(classify_own_kill_text("John用M416击倒了你"))
        self.assertIsNone(classify_own_kill_text("你用ACE32淘汰了[DLTE]SH4_Habibi2协助次数"))
        self.assertIsNone(classify_own_kill_text("你用M416淘汰了caihexiaNB1助攻"))
        self.assertIsNone(classify_own_kill_text("[JOY]B1edAwoA_终于淘汰了你"))
        self.assertIsNone(classify_own_kill_text("你终于淘汰了EnemyPlayer123淘汰数2协助次数"))

    def test_classifies_both_target_kinds(self):
        self.assertEqual(classify_target_kind("John用M416击倒了你", "both"), ("paddle-strict-self-text", "self-death"))
        self.assertEqual(classify_target_kind("你用M416淘汰了Player123", "both"), ("paddle-own-kill-text", "own-kill"))
        self.assertIsNone(classify_target_kind("你用M416淘汰了Player123协助次数", "both"))
        self.assertIsNone(classify_target_kind("你用M416淘汰了Player123助攻", "both"))
        self.assertIsNone(classify_target_kind("你终于淘汰了Player123淘汰数2协助次数", "both"))

    def test_detects_assist_text(self):
        self.assertTrue(has_assist_text("1助攻"))
        self.assertTrue(has_assist_text("协助次数"))
        self.assertFalse(has_assist_text("淘汰数"))
        self.assertTrue(is_assist_own_kill_text("你用M416淘汰了Player123助攻"))
        self.assertFalse(is_assist_own_kill_text("你终于淘汰了Player123淘汰数2协助次数"))
        self.assertFalse(is_assist_own_kill_text("你用BerylM762淘汰了Player123淘汰3协助次数4"))

    def test_detects_delayed_own_elim_text(self):
        self.assertTrue(is_delayed_own_elim_text("你终于淘汰了Player123"))
        self.assertFalse(is_delayed_own_elim_text("你用M416淘汰了Player123"))
        self.assertEqual(
            [event.subject for event in extract_text_events("你用M416击倒了EnemyA你终于淘汰了EnemyA淘汰数1", "both")],
            ["EnemyA"],
        )

    def test_extracts_multiple_own_kill_events(self):
        events = extract_text_events("你用BerylM762击倒了EnemyA你用BerylM762击倒了EnemyB", "both")
        self.assertEqual([event.subject for event in events], ["EnemyA", "EnemyB"])
        self.assertEqual([event.action for event in events], ["knock", "knock"])

    def test_extracts_own_kill_weapon(self):
        event = extract_text_events("你用燃烧弹淘汰了EnemyA淘汰数", "both")[0]
        self.assertEqual(event.action, "eliminate")
        self.assertEqual(event.weapon, "燃烧弹")

    def test_extracts_self_death_weapon(self):
        event = extract_text_events("EnemyA用燃烧瓶淘汰了你", "both")[0]
        self.assertEqual(event.action, "eliminate")
        self.assertEqual(event.weapon, "燃烧瓶")

    def test_does_not_dedupe_short_prefix_as_same_id(self):
        events = extract_text_events("你用BerylM762击倒了[I7]Nvccp你用BerylM762击倒了[I7]NvccpFREBOBE0", "both")
        self.assertFalse(same_text_event(events[0], events[1]))

    def test_dedupes_long_ocr_suffix_noise(self):
        events = extract_text_events("你用自动装填步枪击倒了ASKKZM你用自动装填步枪击倒了ASKKZMPNC2020", "both")
        self.assertTrue(same_noisy_close_event(events[0], events[1]))

    def test_dedupes_close_ocr_suffix_and_digit_noise(self):
        ha0 = extract_text_events("你用ACE32击倒了J0920-HA0", "both")[0]
        haq2 = extract_text_events("你用ACE32击倒了J0920-HAQ2", "both")[0]
        self.assertTrue(same_noisy_close_event(ha0, haq2))

        plain = extract_text_events("你用MP5K淘汰了PainTheGod-淘汰", "both")[0]
        noisy = extract_text_events("你用MP5K淘汰了PainTheGodXW淘汰", "both")[0]
        self.assertTrue(same_noisy_close_event(plain, noisy))

    def test_dedupes_self_death_prefix_noise(self):
        noisy = extract_text_events("JOTDTeuaWUa_m[JOY]B1edawoa_终于淘汰了你", "both")[0]
        clean = extract_text_events("[JOY]B1edawoa_终于淘汰了你", "both")[0]
        self.assertTrue(same_noisy_close_event(noisy, clean))
        noisy_prefix = extract_text_events("[JOTjDTeuawua_mr命中头部击倒门你[JOY]B1edawoa_终于淘汰了你", "both")[0]
        other_noisy_prefix = extract_text_events("[JOrjDTeuawua[JOY]B1edawoa_终于淘汰了你", "both")[0]
        self.assertTrue(same_noisy_close_event(noisy_prefix, other_noisy_prefix))

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
        self.assertEqual(sampled, 3)
        self.assertAlmostEqual(ocr_seconds, 0.03)
        self.assertAlmostEqual(frame_seconds, 0.0)
        self.assertEqual(seen, [29.0, 30.0, 30.5])

    def test_detect_assist_nearby_checks_after_event(self):
        seen: list[float] = []

        def fake_ocr_assist_at(_cv2, _cap, _ocr, sec, _config):
            seen.append(sec)
            if sec >= 31.0:
                return OcrResult("1助攻", "0.99", 0.02, "paddle-own-kill-assist-skipped", 0.01)
            return OcrResult("", "", 0.01, "paddle-not-assist-text", 0.01)

        config = OcrConfig(assist_after=1.2, assist_step=0.5)
        event = extract_text_events("你用M416淘汰了Player123", "both")[0]
        with patch("pubg_highlight_trim.ocr.ocr_assist_at", fake_ocr_assist_at):
            result, sampled, ocr_seconds, frame_seconds = detect_assist_nearby(None, None, None, 30.5, 60.0, config, event)

        self.assertIsNone(result)
        self.assertEqual(sampled, 3)
        self.assertAlmostEqual(ocr_seconds, 0.05)
        self.assertAlmostEqual(frame_seconds, 0.03)
        self.assertEqual(seen, [30.5, 31.0, 31.5])

    def test_detect_assist_nearby_skips_same_subject_assist(self):
        def fake_ocr_assist_at(_cv2, _cap, _ocr, sec, _config):
            if sec >= 31.0:
                return OcrResult("你用M416淘汰了Player123助攻", "0.99", 0.02, "paddle-own-kill-assist-skipped", 0.01)
            return OcrResult("", "", 0.01, "paddle-not-assist-text", 0.01)

        config = OcrConfig(assist_after=1.2, assist_step=0.5)
        event = extract_text_events("你用M416淘汰了Player123", "both")[0]
        with patch("pubg_highlight_trim.ocr.ocr_assist_at", fake_ocr_assist_at):
            result, sampled, ocr_seconds, frame_seconds = detect_assist_nearby(None, None, None, 30.5, 60.0, config, event)

        self.assertIsNotNone(result)
        self.assertEqual(result.method, "paddle-own-kill-assist-skipped")
        self.assertEqual(sampled, 2)
        self.assertAlmostEqual(ocr_seconds, 0.03)
        self.assertAlmostEqual(frame_seconds, 0.02)

    def test_detect_events_keeps_multiple_ids_and_dedupes_repeats(self):
        class FakeCap:
            def release(self):
                pass

        class FakeCv2:
            def VideoCapture(self, _path):
                return FakeCap()

        def fake_ocr_at(_cv2, _cap, _ocr, sec, _config):
            if sec >= 32.0:
                text = "你用BerylM762击倒了EnemyA你用BerylM762击倒了EnemyB"
            elif sec >= 30.0:
                text = "你用BerylM762击倒了EnemyA"
            else:
                text = ""
            method = "paddle-own-kill-text" if text else "paddle-not-target-text"
            return OcrResult(text, "", 0.01, method)

        config = OcrConfig(
            target="both",
            priority_window=[(30.0, 38.0)],
            no_full_scan=True,
            candidate_step=4.0,
            refine_before=4.0,
            refine_search_step=2.0,
            refine_step=0.5,
        )
        with patch("pubg_highlight_trim.ocr.ocr_at", fake_ocr_at), patch(
            "pubg_highlight_trim.ocr.detect_assist_nearby", return_value=(None, 0, 0.0, 0.0)
        ):
            detections = detect_events("dummy.mp4", FakeCv2(), None, 60.0, [], config)

        self.assertEqual(len(detections), 2)
        self.assertEqual([d.event_key for d in detections], ["own-kill:knock:enemya", "own-kill:knock:enemyb"])
        self.assertEqual([d.event_sec for d in detections], [30.0, 32.0])

    def test_detect_events_dedupes_nearby_ocr_suffix_noise(self):
        class FakeCap:
            def release(self):
                pass

        class FakeCv2:
            def VideoCapture(self, _path):
                return FakeCap()

        def fake_ocr_at(_cv2, _cap, _ocr, sec, _config):
            if sec >= 45.0:
                text = "你用自动装填步枪击倒了ASKKZMPNC2020"
            elif sec >= 43.0:
                text = "你用自动装填步枪击倒了ASKKZM"
            else:
                text = ""
            method = "paddle-own-kill-text" if text else "paddle-not-target-text"
            return OcrResult(text, "", 0.01, method)

        config = OcrConfig(
            target="both",
            priority_window=[(43.0, 45.0)],
            no_full_scan=True,
            candidate_step=2.0,
            refine_before=0.0,
            refine_search_step=2.0,
            refine_step=0.5,
        )
        with patch("pubg_highlight_trim.ocr.ocr_at", fake_ocr_at), patch(
            "pubg_highlight_trim.ocr.detect_assist_nearby", return_value=(None, 0, 0.0, 0.0)
        ):
            detections = detect_events("dummy.mp4", FakeCv2(), None, 60.0, [], config)

        self.assertEqual(len(detections), 1)
        self.assertEqual(detections[0].event_key, "own-kill:knock:askkzm")


if __name__ == "__main__":
    unittest.main()
