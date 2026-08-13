from __future__ import annotations

from multiprocessing import freeze_support

from pubg_highlight_trim.cli import main
from pubg_highlight_trim.runtime import configure_process_output_encoding

if __name__ == "__main__":
    configure_process_output_encoding()
    freeze_support()
    raise SystemExit(main())
