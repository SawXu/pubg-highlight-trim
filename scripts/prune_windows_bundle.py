from __future__ import annotations

import argparse
import filecmp
import shutil
from dataclasses import dataclass
from pathlib import Path


@dataclass
class PrunedItem:
    path: Path
    size: int
    reason: str


def format_size(size: int) -> str:
    return f"{size / 1024 / 1024:.1f} MB"


def is_within(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
    except ValueError:
        return False
    return True


def file_size(path: Path) -> int:
    return path.stat().st_size if path.is_file() else sum(file.stat().st_size for file in path.rglob("*") if file.is_file())


def remove_path(path: Path, bundle: Path, reason: str, dry_run: bool, pruned: list[PrunedItem]) -> None:
    if not path.exists():
        return
    if not is_within(path, bundle):
        raise RuntimeError(f"Refusing to remove path outside bundle: {path}")

    size = file_size(path)
    pruned.append(PrunedItem(path, size, reason))
    if dry_run:
        return
    if path.is_dir():
        shutil.rmtree(path)
    else:
        path.unlink()


def prune_duplicate_ffmpeg_dlls(bundle: Path, dry_run: bool, pruned: list[PrunedItem]) -> None:
    internal = bundle / "_internal"
    vendor_ffmpeg = internal / "vendor" / "ffmpeg"
    if not vendor_ffmpeg.exists():
        return

    for vendor_dll in sorted(vendor_ffmpeg.glob("*.dll")):
        top_level_dll = internal / vendor_dll.name
        if not top_level_dll.exists():
            continue
        if not filecmp.cmp(top_level_dll, vendor_dll, shallow=False):
            print(f"Keeping non-identical DLL: {top_level_dll}")
            continue
        remove_path(top_level_dll, bundle, "duplicate FFmpeg DLL already bundled with ffmpeg.exe", dry_run, pruned)


def prune_development_files(bundle: Path, dry_run: bool, pruned: list[PrunedItem]) -> None:
    internal = bundle / "_internal"
    remove_path(internal / "paddle" / "include", bundle, "Paddle C/C++ headers are not used at runtime", dry_run, pruned)

    for pattern, reason in [
        ("paddle/**/*.lib", "Windows import libraries are not used at runtime"),
        ("**/*.pyi", "type stubs are not loaded at runtime"),
        ("cv2/data/haarcascade*.xml", "OpenCV cascade samples are not used by the CLI"),
    ]:
        for path in sorted(internal.glob(pattern)):
            remove_path(path, bundle, reason, dry_run, pruned)


def prune_bundle(bundle: Path, dry_run: bool = False) -> list[PrunedItem]:
    bundle = bundle.resolve()
    if not bundle.exists():
        raise FileNotFoundError(f"Bundle does not exist: {bundle}")

    pruned: list[PrunedItem] = []
    prune_duplicate_ffmpeg_dlls(bundle, dry_run, pruned)
    prune_development_files(bundle, dry_run, pruned)
    return pruned


def main() -> int:
    parser = argparse.ArgumentParser(description="Prune low-risk duplicate and development-only files from a Windows bundle.")
    parser.add_argument("bundle", nargs="?", type=Path, default=Path("dist/pubg-highlight-trim"))
    parser.add_argument("--dry-run", action="store_true", help="Report what would be removed without deleting anything.")
    args = parser.parse_args()

    pruned = prune_bundle(args.bundle, args.dry_run)
    total = sum(item.size for item in pruned)
    action = "Would prune" if args.dry_run else "Pruned"
    print(f"{action} {len(pruned)} item(s), {format_size(total)}")
    for item in sorted(pruned, key=lambda entry: entry.size, reverse=True)[:30]:
        print(f"  {format_size(item.size):>8}  {item.path}  ({item.reason})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
