import unittest

from pubg_highlight_trim.game_languages import game_language_choices, get_game_language_profile


class GameLanguageTests(unittest.TestCase):
    def test_zh_hans_profile_is_the_only_current_profile(self):
        self.assertEqual(game_language_choices(), ["zh-Hans"])

        profile = get_game_language_profile("zh-Hans")

        self.assertEqual(profile.paddle_lang, "ch")
        self.assertTrue(profile.own_kill_event_re.search("你用M416淘汰了Player123"))
        self.assertTrue(profile.own_source_re.search("2026.06.30.淘汰.DVR.mp4"))

    def test_unsupported_game_language_is_rejected(self):
        with self.assertRaises(ValueError):
            get_game_language_profile("en")


if __name__ == "__main__":
    unittest.main()
