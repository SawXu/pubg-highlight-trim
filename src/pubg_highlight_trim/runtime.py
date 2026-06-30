from __future__ import annotations

import sys
from pathlib import Path


def runtime_roots() -> list[Path]:
    roots: list[Path] = []
    frozen_root = getattr(sys, "_MEIPASS", None)
    if frozen_root:
        roots.append(Path(frozen_root))
    if getattr(sys, "frozen", False):
        roots.append(Path(sys.executable).resolve().parent)
    roots.append(Path(__file__).resolve().parents[2])
    roots.append(Path.cwd())

    seen: set[str] = set()
    unique: list[Path] = []
    for root in roots:
        key = str(root).lower()
        if key not in seen:
            seen.add(key)
            unique.append(root)
    return unique


def first_existing_runtime_path(*parts: str) -> Path | None:
    for root in runtime_roots():
        candidate = root.joinpath(*parts)
        if candidate.exists():
            return candidate
    return None
