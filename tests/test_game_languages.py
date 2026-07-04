import unittest
import tempfile
from pathlib import Path

from pubg_highlight_trim.game_languages import game_language_choices, get_game_language_profile
from pubg_highlight_trim.ocr import (
    extract_text_events,
    is_assist_own_kill_text,
    is_delayed_own_elim_text,
    is_molotov_weapon,
)
from pubg_highlight_trim.source_files import iter_source_files


class GameLanguageTests(unittest.TestCase):
    def touch(self, path: Path) -> None:
        path.write_bytes(b"")

    def test_supported_game_language_profiles(self):
        self.assertEqual(game_language_choices(), ["zh-Hans", "zh-Hant"])

        profile = get_game_language_profile("zh-Hans")

        self.assertEqual(profile.paddle_lang, "ch")
        self.assertTrue(profile.own_kill_event_re.search("你用M416淘汰了Player123"))
        self.assertTrue(profile.own_source_re.search("2026.06.30.淘汰.DVR.mp4"))

    def test_zh_hant_extracts_own_kill_and_self_death_events(self):
        profile = get_game_language_profile("zh-Hant")

        own_knock = extract_text_events("您以SLR爆頭擊倒[CNGL]119T", "both", profile)[0]
        own_elim = extract_text_events("您以BerylM762擊殺[NR]Yuet-Wah-Chon2擊殺數", "both", profile)[0]
        self_knock = extract_text_events("[IDOL]Azzzj0500以M416擊倒您", "both", profile)[0]
        self_elim = extract_text_events("BuBy以M16A4擊殺您", "both", profile)[0]

        self.assertEqual(own_knock.key, "own-kill:knock:cngl]119t")
        self.assertEqual(own_knock.weapon, "SLR爆頭")
        self.assertEqual(own_elim.key, "own-kill:eliminate:nr]yuet-wah-chon")
        self.assertEqual(self_knock.target, "self-death")
        self.assertEqual(self_knock.action, "knock")
        self.assertEqual(self_elim.action, "eliminate")

    def test_zh_hant_strips_distance_from_subject_key(self):
        profile = get_game_language_profile("zh-Hant")

        event = extract_text_events("您以BerylM762擊殺heopto(148m)擊殺", "both", profile)[0]

        self.assertEqual(event.key, "own-kill:eliminate:heopto")

    def test_zh_hant_skips_assist_and_delayed_elim_text(self):
        profile = get_game_language_profile("zh-Hant")

        self.assertTrue(is_assist_own_kill_text("您以BerylM762擊殺[NR]JcLelouch助攻", profile))
        self.assertTrue(is_assist_own_kill_text("您以AKM擊殺DASH-P2助殺", profile))
        self.assertFalse(is_assist_own_kill_text("您以P90擊殺[LG]FyyyC3擊殺數", profile))
        self.assertTrue(is_delayed_own_elim_text("您終於擊殺ATOMIC27201擊殺", profile))

    def test_zh_hant_source_filename_profile(self):
        profile = get_game_language_profile("zh-Hant")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self_death = root / "a.死亡.DVR.mp4"
            own_knock = root / "b.擊倒.DVR.mp4"
            own_kill = root / "c.單次擊殺.DVR.mp4"
            match_end = root / "d.對戰結束.DVR.mp4"
            replay = root / "e.死亡畫面.DVR.mp4"
            for path in [self_death, own_knock, own_kill, match_end, replay]:
                self.touch(path)

            self.assertEqual(iter_source_files(root, target="self-death", language=profile), [self_death])
            self.assertEqual(iter_source_files(root, target="own-kill", language=profile), [own_knock, own_kill])
            self.assertEqual(iter_source_files(root, target="both", language=profile), [self_death, own_knock, own_kill, match_end])

    def test_zh_hant_molotov_aliases(self):
        profile = get_game_language_profile("zh-Hant")

        self.assertTrue(is_molotov_weapon("燃燒彈", profile))
        self.assertTrue(is_molotov_weapon("汽油彈", profile))

    def test_unsupported_game_language_is_rejected(self):
        with self.assertRaises(ValueError):
            get_game_language_profile("en")


if __name__ == "__main__":
    unittest.main()
