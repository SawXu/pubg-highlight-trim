from __future__ import annotations

import contextlib
import os
import sys
from collections.abc import Iterator
from pathlib import Path


def configure_process_output_encoding() -> None:
    encoding = os.environ.get("PUBG_HIGHLIGHT_TRIM_OUTPUT_ENCODING")
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            if encoding:
                reconfigure(encoding=encoding, errors="replace")
            else:
                # Redirected Windows consoles can use a legacy code page.
                # A Unicode report path must not turn a completed scan into failure.
                reconfigure(errors="replace")


def ocr_backend_name() -> str:
    backend = os.environ.get("PUBG_OCR_BACKEND", "rust").strip().lower()
    if backend not in {"rust", "paddle"}:
        raise ValueError(
            f"unknown OCR backend {backend!r}; expected rust (default) or paddle (explicit rollback)"
        )
    return backend


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

    # On Windows, dup2/restore can leave multiprocessing spawn with an invalid
    # inherited console handle (notably when launched from nested PowerShell).
    # Redirect Python streams without mutating process-level handles instead.
    # Native child-process output is intentionally not suppressed on this path;
    # Rust OCR diagnostics remain visible, but worker handles stay valid.
    if os.name == "nt":
        with open(os.devnull, "w", encoding="utf-8") as devnull:
            with contextlib.redirect_stdout(devnull), contextlib.redirect_stderr(devnull):
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
    try:
        saved_stdout = os.dup(stdout_fd)
        saved_stderr = os.dup(stderr_fd)
    except OSError:
        with open(os.devnull, "w", encoding="utf-8") as devnull:
            with contextlib.redirect_stdout(devnull), contextlib.redirect_stderr(devnull):
                yield
        return
    try:
        with open(os.devnull, "w", encoding="utf-8") as devnull:
            try:
                os.dup2(devnull.fileno(), stdout_fd)
                os.dup2(devnull.fileno(), stderr_fd)
            except OSError:
                with contextlib.redirect_stdout(devnull), contextlib.redirect_stderr(devnull):
                    yield
            else:
                yield
    finally:
        sys.stdout.flush()
        sys.stderr.flush()
        os.dup2(saved_stdout, stdout_fd)
        os.dup2(saved_stderr, stderr_fd)
        os.close(saved_stdout)
        os.close(saved_stderr)


@contextlib.contextmanager
def suppress_python_output(enabled: bool = True) -> Iterator[None]:
    if not enabled:
        yield
        return
    with open(os.devnull, "w", encoding="utf-8") as devnull:
        with contextlib.redirect_stdout(devnull), contextlib.redirect_stderr(devnull):
            yield
