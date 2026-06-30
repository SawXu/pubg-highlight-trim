from __future__ import annotations

import re
from pathlib import Path

OWN_SOURCE_RE = re.compile(r"\.(?:被击倒|淘汰)\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE)
VIEW_REPLAY_RE = re.compile(r"(?:淘汰画面|击倒画面)")
ANY_KNOCK_ELIM_RE = re.compile(r"(?:淘汰|击倒).*\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE)


def iter_source_files(folder: Path, include_view_replays: bool = False, recursive: bool = False) -> list[Path]:
    pattern = "**/*.mp4" if recursive else "*.mp4"
    files: list[Path] = []
    for path in sorted(folder.glob(pattern), key=lambda p: str(p).lower()):
        name = path.name
        if include_view_replays:
            if ANY_KNOCK_ELIM_RE.search(name):
                files.append(path)
            continue
        if OWN_SOURCE_RE.search(name) and not VIEW_REPLAY_RE.search(name):
            files.append(path)
    return files
