from __future__ import annotations

import argparse
import platform
from pathlib import Path

from . import __version__
from .pipeline import run


def parse_roi(value: str) -> tuple[float, float, float, float]:
    try:
        parts = [float(part.strip()) for part in value.split(",")]
    except ValueError as exc:
        raise argparse.ArgumentTypeError("ROI must be x1,y1,x2,y2") from exc
    if len(parts) != 4:
        raise argparse.ArgumentTypeError("ROI must be x1,y1,x2,y2")
    x1, y1, x2, y2 = parts
    if not (0 <= x1 < x2 <= 1 and 0 <= y1 < y2 <= 1):
        raise argparse.ArgumentTypeError("ROI values must be ratios in ascending order, between 0 and 1")
    return x1, y1, x2, y2


def parse_window(value: str) -> tuple[float, float]:
    if ":" not in value:
        raise argparse.ArgumentTypeError("Window must be start:end seconds")
    try:
        start, end = (float(part.strip()) for part in value.split(":", 1))
    except ValueError as exc:
        raise argparse.ArgumentTypeError("Window must be start:end seconds") from exc
    if start < 0 or end <= start:
        raise argparse.ArgumentTypeError("Window must satisfy 0 <= start < end")
    return start, end


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="pubg-highlight-trim",
        description="Trim PUBG NVIDIA Highlight clips to the player's own knock/elimination moment.",
    )
    parser.add_argument("input", nargs="?", type=Path, default=Path("."), help="PUBG highlight folder or a single mp4 file")
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    parser.add_argument("--detector", choices=["auto", "ocr", "health"], default="auto", help="auto tries OCR first, then health fallback")
    parser.add_argument(
        "--target",
        choices=["self-death", "own-kill", "both"],
        default="self-death",
        help="Text event to detect: self-death for enemies knocking/eliminating you, own-kill for you knocking/eliminating others, both for both kinds",
    )
    parser.add_argument("--output-dir", type=Path, default=None, help="Directory for individual trimmed clips")
    parser.add_argument("--final", type=Path, default=None, help="Merged montage mp4 path")
    parser.add_argument("--before", "--seconds-before", dest="seconds_before", type=float, default=4.0, help="Seconds to keep before event")
    parser.add_argument("--after", "--seconds-after", dest="seconds_after", type=float, default=1.0, help="Seconds to keep after event")
    parser.add_argument("--min-event-sec", type=float, default=2.0, help="Skip detected events earlier than this many seconds; use 0 to keep opening events")
    parser.add_argument("--include-view-replays", action="store_true", help="Also include replay-perspective highlight files")
    parser.add_argument("--recursive", action="store_true", help="Search subdirectories too")
    parser.add_argument("--dry-run", action="store_true", help="Detect and write CSV/summary without trimming or merging")
    parser.add_argument("--no-merge", action="store_true", help="Create individual clips but skip final concat")
    parser.add_argument("--overwrite", action="store_true", help="Overwrite the selected output directory/final file instead of creating unique names")
    parser.add_argument("--profile", action="store_true", help="Print per-clip timing breakdown for OCR, frame reads, health checks, and trimming")
    parser.add_argument("--allow-starts-downed", action="store_true", help="Do not skip clips whose opening already has a red downed health bar")
    parser.add_argument("--ffmpeg", default=None, help="Explicit ffmpeg.exe path")
    parser.add_argument("--ffprobe", default=None, help="Explicit ffprobe.exe path")

    ocr = parser.add_argument_group("OCR options")
    ocr.add_argument("--candidate-csv", type=Path, default=None, help="Optional prior CSV; EventSec values are used as scan hints")
    ocr.add_argument("--priority-window", type=parse_window, action="append", default=[(31.0, 43.0), (45.0, 53.0)], help="Scan this OCR window first; repeatable; default 31:43 and 45:53")
    ocr.add_argument("--scan-start", type=float, default=0.0)
    ocr.add_argument("--scan-end", type=float, default=None)
    ocr.add_argument("--coarse-step", type=float, default=4.0)
    ocr.add_argument("--candidate-lookback", type=float, default=8.0)
    ocr.add_argument("--candidate-lookahead", type=float, default=0.5)
    ocr.add_argument("--candidate-step", type=float, default=4.0)
    ocr.add_argument("--refine-before", type=float, default=6.0)
    ocr.add_argument("--refine-after", type=float, default=0.4)
    ocr.add_argument("--refine-step", type=float, default=0.5)
    ocr.add_argument("--no-full-scan", action="store_true", help="Only scan candidate/priority windows; faster but can miss unusual timings")
    ocr.add_argument("--roi", type=parse_roi, default=(0.30, 0.66, 0.70, 0.75), help="OCR crop ratios x1,y1,x2,y2")
    ocr.add_argument("--ocr-width", type=int, default=768, help="Downscale OCR ROI to this width; 0 disables")

    opening = parser.add_argument_group("opening downed check")
    opening.add_argument("--opening-check-start", type=float, default=0.5)
    opening.add_argument("--opening-check-end", type=float, default=3.0)
    opening.add_argument("--opening-check-fps", type=float, default=5.0)
    opening.add_argument("--opening-red-threshold", type=float, default=0.65)
    return parser


def main(argv: list[str] | None = None) -> int:
    if platform.system() != "Windows":
        raise SystemExit("pubg-highlight-trim currently supports Windows only.")
    args = build_parser().parse_args(argv)
    return run(args)
