import tempfile
import unittest
from pathlib import Path

from pubg_highlight_trim.source_files import iter_source_files


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

    def test_iter_source_files_can_include_view_replays(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            own_elim = root / "a.淘汰.DVR.mp4"
            replay = root / "b.淘汰画面.DVR.mp4"
            for path in [own_elim, replay]:
                self.touch(path)

            self.assertEqual(iter_source_files(root, include_view_replays=True), [own_elim, replay])

    def test_iter_source_files_can_select_own_kill_highlights(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            own_kill = root / "a.单次淘汰.DVR.mp4"
            multi_kill = root / "b.双次淘汰.DVR.mp4"
            self_death = root / "c.淘汰.DVR.mp4"
            for path in [own_kill, multi_kill, self_death]:
                self.touch(path)

            self.assertEqual(iter_source_files(root, target="own-kill"), [own_kill, multi_kill])

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


if __name__ == "__main__":
    unittest.main()
