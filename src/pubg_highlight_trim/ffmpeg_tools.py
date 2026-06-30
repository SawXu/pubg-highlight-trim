from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

from .runtime import runtime_roots


def _bundled_dirs() -> list[Path]:
    dirs: list[Path] = []
    for root in runtime_roots():
        dirs.extend(
            [
                root / "vendor" / "ffmpeg",
                root / "ffmpeg",
                root / "_internal" / "vendor" / "ffmpeg",
            ]
        )
    return _dedupe_dirs(dirs)


def _fallback_dirs() -> list[Path]:
    dirs: list[Path] = []
    env_dir = os.environ.get("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR")
    if env_dir:
        dirs.append(Path(env_dir))
    dirs.extend(
        [
            Path(r"C:\Program Files\Shutter Encoder\app\Library"),
            Path(r"C:\Program Files\Shutter Encoder\Library"),
        ]
    )
    return _dedupe_dirs(dirs)


def _dedupe_dirs(dirs: list[Path]) -> list[Path]:
    seen: set[str] = set()
    unique: list[Path] = []
    for directory in dirs:
        key = str(directory).lower()
        if key not in seen:
            seen.add(key)
            unique.append(directory)
    return unique


def find_binary(name: str, explicit: str | None = None) -> str:
    if explicit:
        path = Path(explicit)
        if path.exists():
            return str(path)
        raise FileNotFoundError(f"Explicit {name} path does not exist: {path}")

    for directory in _bundled_dirs():
        candidate = directory / name
        if candidate.exists():
            return str(candidate)

    env_dir = os.environ.get("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR")
    if env_dir:
        candidate = Path(env_dir) / name
        if candidate.exists():
            return str(candidate)

    found = shutil.which(name)
    if found:
        return found

    for directory in _fallback_dirs():
        candidate = directory / name
        if candidate.exists():
            return str(candidate)

    raise FileNotFoundError(
        f"Could not find {name}. Release builds should bundle it; for source runs pass --{name[:-4]} or set PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR."
    )


def find_ffmpeg_pair(ffmpeg: str | None = None, ffprobe: str | None = None) -> tuple[str, str]:
    return find_binary("ffmpeg.exe", ffmpeg), find_binary("ffprobe.exe", ffprobe)


def run_stdout(cmd: list[str]) -> str:
    return subprocess.check_output(cmd, text=True, stderr=subprocess.PIPE).strip()


def duration_sec(path: Path, ffprobe: str) -> float:
    return float(
        run_stdout(
            [
                ffprobe,
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=nw=1:nk=1",
                str(path),
            ]
        )
    )


def trim_clip(src: Path, out: Path, start: float, length: float, ffmpeg: str) -> str:
    out.parent.mkdir(parents=True, exist_ok=True)
    nvenc_cmd = [
        ffmpeg,
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-ss",
        f"{start:.3f}",
        "-i",
        str(src),
        "-t",
        f"{length:.3f}",
        "-map",
        "0:v:0",
        "-map",
        "0:a:0?",
        "-c:v",
        "h264_nvenc",
        "-preset",
        "p4",
        "-rc",
        "vbr",
        "-cq",
        "22",
        "-b:v",
        "0",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-movflags",
        "+faststart",
        str(out),
    ]
    try:
        subprocess.run(nvenc_cmd, check=True)
        return "h264_nvenc"
    except Exception:
        fallback = [
            ffmpeg,
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-ss",
            f"{start:.3f}",
            "-i",
            str(src),
            "-t",
            f"{length:.3f}",
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "20",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            str(out),
        ]
        subprocess.run(fallback, check=True)
        return "libx264"


def concat_clips(clips: list[Path], final: Path, ffmpeg: str) -> Path:
    final.parent.mkdir(parents=True, exist_ok=True)
    list_path = final.with_suffix(".concat.txt")
    with list_path.open("w", encoding="utf-8") as handle:
        for clip in clips:
            escaped = str(clip.resolve()).replace("'", "'\\''")
            handle.write(f"file '{escaped}'\n")
    subprocess.run(
        [
            ffmpeg,
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(list_path),
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            str(final),
        ],
        check=True,
    )
    return list_path
