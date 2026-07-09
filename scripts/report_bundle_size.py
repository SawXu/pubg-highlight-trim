from __future__ import annotations

import argparse
from collections import defaultdict
from pathlib import Path


def format_size(size: int) -> str:
    return f"{size / 1024 / 1024:.1f} MB"


def bundle_group(bundle: Path, path: Path) -> str:
    relative = path.relative_to(bundle)
    if len(relative.parts) >= 2 and relative.parts[0] == "_internal":
        return relative.parts[1]
    return relative.parts[0]


def iter_files(root: Path) -> list[Path]:
    return [path for path in root.rglob("*") if path.is_file()]


def main() -> int:
    parser = argparse.ArgumentParser(description="Print a size report for a PyInstaller Windows bundle.")
    parser.add_argument("bundle", nargs="?", type=Path, default=Path("dist/pubg-highlight-trim"))
    parser.add_argument("--zip", dest="zip_path", type=Path, default=Path("dist/pubg-highlight-trim-windows-x64.zip"))
    parser.add_argument("--top", type=int, default=20)
    args = parser.parse_args()

    bundle = args.bundle
    if not bundle.exists():
        raise FileNotFoundError(f"Bundle does not exist: {bundle}")

    files = [(path.stat().st_size, path) for path in iter_files(bundle)]
    total = sum(size for size, _ in files)
    groups: dict[str, int] = defaultdict(int)
    for size, path in files:
        groups[bundle_group(bundle, path)] += size

    print(f"Bundle: {bundle} ({format_size(total)})")
    if args.zip_path.exists():
        print(f"Zip: {args.zip_path} ({format_size(args.zip_path.stat().st_size)})")

    print("\nTop groups:")
    for group, size in sorted(groups.items(), key=lambda item: item[1], reverse=True)[: args.top]:
        print(f"  {format_size(size):>8}  {group}")

    print("\nTop files:")
    for size, path in sorted(files, reverse=True)[: args.top]:
        print(f"  {format_size(size):>8}  {path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
