import tempfile
import unittest
from pathlib import Path

from pubg_highlight_trim.source_files import infer_source_file_languages, iter_source_file_languages, iter_source_files


class SourceFilesTests(unittest.TestCase):
    def touch(self, path: Path) -> None:
        path.write_bytes(b"")

    def test_iter_source_files_excludes_replays_and_multi_kill(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            own_elim = root / "2026.06.30-12.00.00.淘汰.DVR.mp4"
            own_knock = root / "2026.06.30-12.01.00.被击倒.DVR_1.mp4"
            replay = root / "2026.06.30-12.02.00.淘汰画面.DVR.mp4"
            multi = root / "2026.06.30-12.03.00.单次淘汰.DVR.mp4"
            for path in [own_elim, own_knock, replay, multi]:
                self.touch(path)

            self.assertEqual(iter_source_files(root), [own_elim, own_knock])

    def test_iter_source_files_can_select_own_kill_highlights(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            own_kill = root / "a.单次淘汰.DVR.mp4"
            multi_kill = root / "b.双次淘汰.DVR.mp4"
            self_death = root / "c.淘汰.DVR.mp4"
            match_end = root / "d.比赛结束.DVR.mp4"
            for path in [own_kill, multi_kill, self_death, match_end]:
                self.touch(path)

            self.assertEqual(iter_source_files(root, target="own-kill"), [own_kill, multi_kill, match_end])

    def test_iter_source_files_can_select_both_highlight_types(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            own_elim = root / "a.淘汰.DVR.mp4"
            own_kill = root / "b.单次淘汰.DVR.mp4"
            match_end = root / "c.比赛结束.DVR.mp4"
            replay = root / "d.淘汰画面.DVR.mp4"
            for path in [own_elim, own_kill, match_end, replay]:
                self.touch(path)

            self.assertEqual(iter_source_files(root, target="both"), [own_elim, own_kill, match_end])

    def test_iter_source_file_languages_detects_mixed_profiles(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            zh_hans = root / "a.单次淘汰.DVR.mp4"
            zh_hant = root / "b.單次擊殺.DVR.mp4"
            english = root / "c.Single kill.DVR.mp4"
            replay = root / "d.DeathCam.DVR.mp4"
            for path in [zh_hans, zh_hant, english, replay]:
                self.touch(path)

            selections = iter_source_file_languages(root, target="both")

        self.assertEqual(
            [(path.name, profile.code) for path, profile in selections],
            [
                ("a.单次淘汰.DVR.mp4", "zh-Hans"),
                ("b.單次擊殺.DVR.mp4", "zh-Hant"),
                ("c.Single kill.DVR.mp4", "en"),
            ],
        )

    def test_auto_language_detection_uses_context_for_ambiguous_names(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            explicit_hant = root / "a.單次擊殺.DVR.mp4"
            ambiguous = root / "b.淘汰.DVR.mp4"
            for path in [explicit_hant, ambiguous]:
                self.touch(path)

            selections = infer_source_file_languages([explicit_hant, ambiguous], target="both")

        self.assertEqual(selections[explicit_hant].code, "zh-Hant")
        self.assertEqual(selections[ambiguous].code, "zh-Hant")

    def test_auto_language_detection_defaults_ambiguous_names_to_zh_hans(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            ambiguous = root / "a.淘汰.DVR.mp4"
            self.touch(ambiguous)

            selections = infer_source_file_languages([ambiguous], target="both")

        self.assertEqual(selections[ambiguous].code, "zh-Hans")

    def test_auto_language_detection_uses_match_end_for_own_kill(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            end_of_match = root / "a.End of match.DVR.mp4"
            self.touch(end_of_match)

            selections = infer_source_file_languages([end_of_match], target="own-kill")

        self.assertEqual(selections[end_of_match].code, "en")


if __name__ == "__main__":
    unittest.main()
