from __future__ import annotations

import contextlib
import os
import sys
from collections.abc import Iterator
from pathlib import Path


def configure_process_output_encoding() -> None:
    encoding = os.environ.get("PUBG_HIGHLIGHT_TRIM_OUTPUT_ENCODING")
    if not encoding:
        return

    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            reconfigure(encoding=encoding, errors="replace")


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


@contextlib.contextmanager
def suppress_process_output(enabled: bool = True) -> Iterator[None]:
    if not enabled:
        yield
        return

    try:
        stdout_fd = sys.stdout.fileno()
        stderr_fd = sys.stderr.fileno()
    except (AttributeError, OSError):
        with open(os.devnull, "w", encoding="utf-8") as devnull:
            with contextlib.redirect_stdout(devnull), contextlib.redirect_stderr(devnull):
                yield
        return

    sys.stdout.flush()
    sys.stderr.flush()
    saved_stdout = os.dup(stdout_fd)
    saved_stderr = os.dup(stderr_fd)
    try:
        with open(os.devnull, "w", encoding="utf-8") as devnull:
            os.dup2(devnull.fileno(), stdout_fd)
            os.dup2(devnull.fileno(), stderr_fd)
            yield
    finally:
        sys.stdout.flush()
        sys.stderr.flush()
        os.dup2(saved_stdout, stdout_fd)
        os.dup2(saved_stderr, stderr_fd)
        os.close(saved_stdout)
        os.close(saved_stderr)
