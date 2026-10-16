"""Verify the offline OCR model directory and ONNX graph contracts."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=Path("rust/ocr_engine/models/pp_ocrv6_medium.json"))
    parser.add_argument("--check-onnx", action="store_true")
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    failures: list[str] = []
    contract = manifest.get("contract", {})
    for role in ("detector", "recognizer", "dictionary"):
        entry = manifest[role]
        path = args.model_dir / entry["file"]
        if not path.is_file():
            failures.append(f"missing {role}: {path}")
            continue
        actual = sha256(path)
        expected = entry["sha256"].lower()
        if len(expected) != 64 or actual != expected:
            failures.append(f"{role} sha256 mismatch: expected {expected or '<unset>'}, actual {actual}")
        if role == "dictionary":
            try:
                dictionary = path.read_text(encoding="utf-8")
                symbols = {line for line in dictionary.splitlines() if line}
                if len(symbols) < 100:
                    failures.append(f"dictionary content is unexpectedly small: {len(symbols)} symbols")
            except UnicodeDecodeError as exc:
                failures.append(f"dictionary is not valid UTF-8: {exc}")
        if args.check_onnx and role in {"detector", "recognizer"}:
            try:
                import onnx  # type: ignore
                graph = onnx.load(str(path))
                onnx.checker.check_model(graph)
                expected_input = contract.get(f"{role}_input")
                expected_output = contract.get(f"{role}_output")
                if expected_input and graph.graph.input[0].name != expected_input:
                    failures.append(f"{role} input mismatch: expected {expected_input}, actual {graph.graph.input[0].name}")
                if expected_output and graph.graph.output[0].name != expected_output:
                    failures.append(f"{role} output mismatch: expected {expected_output}, actual {graph.graph.output[0].name}")
            except ImportError:
                failures.append("--check-onnx requires the onnx package")
            except Exception as exc:  # pragma: no cover - depends on external graph
                failures.append(f"{role} ONNX checker failed: {exc}")
    if failures:
        for failure in failures:
            print(f"ERROR {failure}")
        return 1
    print(f"OK model_version={manifest['model_version']} provider=cpu")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
