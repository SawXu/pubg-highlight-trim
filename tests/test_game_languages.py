import unittest
import tempfile
from pathlib import Path

from pubg_highlight_trim.game_language_tables import LANGUAGE_REGEX_TABLES
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
        self.assertEqual(game_language_choices(), ["en", "zh-Hans", "zh-Hant"])

        profile = get_game_language_profile("zh-Hans")

        self.assertEqual(profile.paddle_lang, "ch")
        self.assertTrue(profile.own_kill_event_re.search("你用M416淘汰了Player123"))
        self.assertTrue(profile.own_source_re.search("2026.06.30.淘汰.DVR.mp4"))

    def test_language_profiles_are_built_from_regex_tables(self):
        self.assertEqual([table.code for table in LANGUAGE_REGEX_TABLES], ["zh-Hans", "zh-Hant", "en"])

        for table in LANGUAGE_REGEX_TABLES:
            profile = get_game_language_profile(table.code)
            self.assertIs(profile.regex_table, table)
            self.assertTrue(profile.own_kill_event_re.pattern)

    def test_common_assist_filter_uses_language_tables(self):
        cases = [
            ("zh-Hans", "你用M416淘汰了OpponentAlpha助攻"),
            ("zh-Hant", "您以M416擊殺OpponentAlpha助攻"),
            ("en", "YOU KILLED OpponentAlpha with M416 1 ASSIST"),
        ]

        for language, text in cases:
            with self.subTest(language=language):
                profile = get_game_language_profile(language)
                self.assertTrue(is_assist_own_kill_text(text, profile))
                self.assertEqual(extract_text_events(text, "both", profile), [])

    def test_en_extracts_own_kill_and_self_death_events(self):
        profile = get_game_language_profile("en")

        own_knock = extract_text_events("YOU KNOCKED OUT OpponentAlpha with Beryl M762", "both", profile)[0]
        own_elim = extract_text_events("YOU KILLED OpponentBravo with Beryl M762 2 KILLS", "both", profile)[0]
        self_knock = extract_text_events("OpponentCharlie KNOCKED YOU out with ACE32", "both", profile)[0]
        self_elim = extract_text_events("OpponentDelta KILLS YOU with MP5K", "both", profile)[0]

        self.assertEqual(own_knock.key, "own-kill:knock:opponentalpha")
        self.assertEqual(own_knock.weapon, "BerylM762")
        self.assertEqual(own_elim.key, "own-kill:eliminate:opponentbravo")
        self.assertEqual(own_elim.weapon, "BerylM762")
        self.assertEqual(self_knock.target, "self-death")
        self.assertEqual(self_knock.action, "knock")
        self.assertEqual(self_knock.weapon, "ACE32")
        self.assertEqual(self_elim.action, "eliminate")
        self.assertEqual(self_elim.weapon, "MP5K")

    def test_en_skips_delayed_finally_killed_text(self):
        profile = get_game_language_profile("en")

        self.assertTrue(is_delayed_own_elim_text("YOU finally KILLED OpponentAlpha", profile))
        events = extract_text_events(
            "YOU KNOCKED OUT EnemyA with M416 YOU finally KILLED EnemyA 1 KILL",
            "both",
            profile,
        )

        self.assertEqual([event.subject for event in events], ["EnemyA"])

    def test_en_skips_assist_text(self):
        profile = get_game_language_profile("en")

        text = "YOU KILLED OpponentAlpha with M16A4 1 ASSIST"

        self.assertTrue(is_assist_own_kill_text(text, profile))
        self.assertEqual(extract_text_events(text, "both", profile), [])

    def test_zh_hant_extracts_own_kill_and_self_death_events(self):
        profile = get_game_language_profile("zh-Hant")

        own_knock = extract_text_events("您以SLR爆頭擊倒OpponentAlpha", "both", profile)[0]
        own_elim = extract_text_events("您以BerylM762擊殺OpponentBravo2擊殺數", "both", profile)[0]
        self_knock = extract_text_events("OpponentCharlie以M416擊倒您", "both", profile)[0]
        self_elim = extract_text_events("OpponentDelta以M16A4擊殺您", "both", profile)[0]

        self.assertEqual(own_knock.key, "own-kill:knock:opponentalpha")
        self.assertEqual(own_knock.weapon, "SLR爆頭")
        self.assertEqual(own_elim.key, "own-kill:eliminate:opponentbravo")
        self.assertEqual(self_knock.target, "self-death")
        self.assertEqual(self_knock.action, "knock")
        self.assertEqual(self_elim.action, "eliminate")

    def test_zh_hant_strips_distance_from_subject_key(self):
        profile = get_game_language_profile("zh-Hant")

        event = extract_text_events("您以BerylM762擊殺OpponentAlpha(148m)擊殺", "both", profile)[0]

        self.assertEqual(event.key, "own-kill:eliminate:opponentalpha")

    def test_zh_hant_skips_assist_and_delayed_elim_text(self):
        profile = get_game_language_profile("zh-Hant")

        self.assertTrue(is_assist_own_kill_text("您以BerylM762擊殺OpponentAlpha助攻", profile))
        self.assertTrue(is_assist_own_kill_text("您以AKM擊殺OpponentBravo助殺", profile))
        self.assertFalse(is_assist_own_kill_text("您以P90擊殺OpponentCharlie擊殺數", profile))
        self.assertTrue(is_delayed_own_elim_text("您終於擊殺OpponentDelta擊殺", profile))

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

    def test_en_source_filename_profile(self):
        profile = get_game_language_profile("en")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self_knock = root / "a.Knockouted.DVR.mp4"
            self_death = root / "b.Death.DVR.mp4"
            own_knock = root / "c.Knockout.DVR.mp4"
            own_kill = root / "d.Single kill.DVR.mp4"
            multi_kill = root / "e.Double kill.DVR.mp4"
            match_end = root / "f.Match end.DVR.mp4"
            replay = root / "g.Death cam.DVR.mp4"
            for path in [self_knock, self_death, own_knock, own_kill, multi_kill, match_end, replay]:
                self.touch(path)

            self.assertEqual(iter_source_files(root, target="self-death", language=profile), [self_knock, self_death])
            self.assertEqual(iter_source_files(root, target="own-kill", language=profile), [own_knock, own_kill, multi_kill])
            self.assertEqual(
                iter_source_files(root, target="both", language=profile),
                [self_knock, self_death, own_knock, own_kill, multi_kill, match_end],
            )

    def test_zh_hant_molotov_aliases(self):
        profile = get_game_language_profile("zh-Hant")

        self.assertTrue(is_molotov_weapon("燃燒彈", profile))
        self.assertTrue(is_molotov_weapon("汽油彈", profile))

    def test_unsupported_game_language_is_rejected(self):
        with self.assertRaises(ValueError):
            get_game_language_profile("ja")


if __name__ == "__main__":
    unittest.main()
