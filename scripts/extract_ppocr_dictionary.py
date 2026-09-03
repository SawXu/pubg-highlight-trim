"""Extract the effective CTC dictionary from a pinned PP-OCR inference YAML."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from urllib.request import urlopen

import yaml


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expected-sha256")
    args = parser.parse_args()
    with urlopen(args.source, timeout=60) as response:
        document = yaml.safe_load(response.read())
    content = "\n".join(document["PostProcess"]["character_dict"]) + "\n"
    digest = hashlib.sha256(content.encode("utf-8")).hexdigest()
    if args.expected_sha256 and digest != args.expected_sha256.lower():
        raise SystemExit(f"sha256 mismatch: expected {args.expected_sha256}, actual {digest}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(content.encode("utf-8"))
    print(json.dumps({"entries": len(document["PostProcess"]["character_dict"]), "sha256": digest}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
