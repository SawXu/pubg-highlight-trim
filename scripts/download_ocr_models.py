"""Fetch pinned OCR assets during build time; runtime code never downloads them."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
from urllib.request import urlopen

import yaml


def download(url: str, destination: Path, expected: str) -> str:
    if destination.is_file():
        actual = hashlib.sha256(destination.read_bytes()).hexdigest()
        if actual == expected.lower():
            return actual
    digest = hashlib.sha256()
    temporary = destination.with_suffix(destination.suffix + ".part")
    with urlopen(url, timeout=60) as response, temporary.open("wb") as output:
        while chunk := response.read(1024 * 1024):
            output.write(chunk)
            digest.update(chunk)
    actual = digest.hexdigest()
    if actual != expected.lower():
        temporary.unlink(missing_ok=True)
        raise RuntimeError(f"sha256 mismatch for {destination.name}: expected {expected}, actual {actual}")
    os.replace(temporary, destination)
    return actual


def download_dictionary(entry: dict, destination: Path) -> str:
    if destination.is_file():
        actual = hashlib.sha256(destination.read_bytes()).hexdigest()
        if actual == entry["sha256"].lower():
            return actual
    with urlopen(entry["source"], timeout=60) as response:
        document = yaml.safe_load(response.read())
    characters = document["PostProcess"]["character_dict"]
    temporary = destination.with_suffix(destination.suffix + ".part")
    temporary.write_bytes(("\n".join(characters) + "\n").encode("utf-8"))
    actual = hashlib.sha256(temporary.read_bytes()).hexdigest()
    if actual != entry["sha256"].lower():
        temporary.unlink(missing_ok=True)
        raise RuntimeError(f"sha256 mismatch for {destination.name}: expected {entry['sha256']}, actual {actual}")
    os.replace(temporary, destination)
    return actual


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=Path("rust/ocr_engine/models/pp_ocrv6_medium.json"))
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for role in ("detector", "recognizer", "dictionary"):
        entry = manifest[role]
        path = args.output_dir / entry["file"]
        print(f"Downloading {role} from {entry['source']}")
        if entry.get("extract") == "PostProcess.character_dict":
            actual = download_dictionary(entry, path)
        else:
            actual = download(entry["source"], path, entry["sha256"])
        print(f"sha256={actual}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
