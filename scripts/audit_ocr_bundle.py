"""Reject undeclared Paddle OCR runtime residue in the default Windows bundle."""
from __future__ import annotations

import argparse
from pathlib import Path
import zipfile


def paddle_residue(bundle: Path) -> list[str]:
    def forbidden(name: str) -> bool:
        parts = name.replace("\\", "/").lower().split("/")
        return any(
            part in {"paddle", "paddleocr", "paddlex", "paddlex_cache"}
            or part.startswith(("paddleocr-", "paddlepaddle-", "paddlex-", "libpaddle"))
            for part in parts
        )

    residue = []
    for path in bundle.rglob("*"):
        relative = path.relative_to(bundle).as_posix()
        if forbidden(relative):
            residue.append(relative)
        if path.is_file() and path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                residue.extend(f"{relative}!{name}" for name in archive.namelist() if forbidden(name))
    return sorted(residue)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    args = parser.parse_args()
    if not args.bundle.is_dir():
        parser.error(f"bundle not found: {args.bundle}")
    residue = paddle_residue(args.bundle)
    if residue:
        parser.exit(1, "Unexpected Paddle OCR runtime files:\n" + "\n".join(residue) + "\n")
    print("Default OCR bundle audit OK: no Paddle packages or model cache")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
