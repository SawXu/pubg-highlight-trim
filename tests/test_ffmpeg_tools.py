import tempfile
import unittest
from pathlib import Path

from pubg_highlight_trim.ffmpeg_tools import find_binary


class FfmpegToolsTests(unittest.TestCase):
    def test_find_binary_uses_explicit_path(self):
        with tempfile.TemporaryDirectory() as temp:
            exe = Path(temp) / "ffmpeg.exe"
            exe.write_bytes(b"fake")
            self.assertEqual(find_binary("ffmpeg.exe", str(exe)), str(exe))


if __name__ == "__main__":
    unittest.main()
