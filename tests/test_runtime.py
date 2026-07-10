from __future__ import annotations

import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO

from pubg_highlight_trim.runtime import suppress_process_output


class SuppressProcessOutputTests(unittest.TestCase):
    def test_suppresses_python_and_native_output_then_restores_streams(self):
        script = textwrap.dedent(
            """
            import os
            import sys
            from pubg_highlight_trim.runtime import suppress_process_output

            print("before", flush=True)
            with suppress_process_output():
                print("python noise", flush=True)
                os.write(sys.stdout.fileno(), b"native noise\\n")
                os.write(sys.stderr.fileno(), b"native error\\n")
            print("after", flush=True)
            """
        )

        env = os.environ.copy()
        src_dir = str(Path(__file__).resolve().parents[1] / "src")
        env["PYTHONPATH"] = os.pathsep.join(filter(None, [src_dir, env.get("PYTHONPATH", "")]))
        result = subprocess.run(
            [sys.executable, "-c", script],
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )

        self.assertEqual(result.stdout, "before\nafter\n")
        self.assertEqual(result.stderr, "")

    def test_supports_streams_without_file_descriptors(self):
        stdout = StringIO()
        stderr = StringIO()

        with redirect_stdout(stdout), redirect_stderr(stderr):
            with suppress_process_output():
                print("noise")

        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(stderr.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
