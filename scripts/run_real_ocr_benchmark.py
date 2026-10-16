"""Run the Rust OCR CLI against the checked-in real ROI fixture."""
from __future__ import annotations

import argparse
import base64
import json
import re
import subprocess
from pathlib import Path
import sys

import cv2

sys.path.insert(0, str(Path(__file__).parents[1]))
from scripts.build_real_ocr_benchmark import action, event_kind


def normalize(value: str) -> str:
    return re.sub(r"\s+", "", value or "")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, default=Path("rust/ocr_engine/fixtures/real"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    rows = [json.loads(line) for line in (args.fixture / "baseline.jsonl").read_text(encoding="utf-8").splitlines()]
    requests = []
    for row in rows:
        image = cv2.imread(str(args.fixture / row["image"]))
        rgb = cv2.cvtColor(image, cv2.COLOR_BGR2RGB)
        requests.append(json.dumps({"pixels_b64": base64.b64encode(rgb.tobytes()).decode("ascii"), "width": int(rgb.shape[1]), "height": int(rgb.shape[0])}))
    process = subprocess.Popen([str(args.executable), str(args.model_dir)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")
    output, error = process.communicate("\n".join(requests) + "\n", timeout=300)
    if process.returncode != 0:
        raise SystemExit(error.strip() or f"Rust OCR exited with {process.returncode}")
    results = [json.loads(line) for line in output.splitlines()]
    if len(results) != len(rows):
        raise SystemExit(f"expected {len(rows)} results, got {len(results)}")
    python_output = args.output.with_name("python.jsonl")
    rust_output = args.output.with_name("rust.jsonl")
    python_output.write_text("\n".join(json.dumps(row, ensure_ascii=False) for row in rows) + "\n", encoding="utf-8")
    rust_rows = []
    for row, result in zip(rows, results):
        rust_rows.append({**result, "frame_id": row["frame_id"], "action": action(result.get("text", "")), "event_kind": event_kind(result.get("text", "")), "target": row["target"]})
    rust_output.write_text("\n".join(json.dumps(row, ensure_ascii=False) for row in rust_rows) + "\n", encoding="utf-8")
    comparison = subprocess.run(["python", "scripts/compare_ocr_baselines.py", "--allow-missing-boxes", str(python_output), str(rust_output)], capture_output=True, text=True, check=False)
    if comparison.returncode not in (0, 1):
        raise SystemExit(comparison.stderr.strip() or "OCR baseline comparator failed to run")
    comparison_report = json.loads(comparison.stdout)
    text_matches = sum(normalize(left["text"]) == normalize(right.get("text", "")) for left, right in zip(rows, results))
    if any(right.get("status") != "ok" or not right.get("boxes") for right in results):
        raise SystemExit("real ROI smoke produced an invalid status or empty detection boxes")
    report = {"frames": len(rows), "text_match_rate": text_matches / len(rows), "all_status_ok": True, "all_boxes_nonempty": True, "comparison_exit_code": comparison.returncode, "comparison": comparison_report}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(json.dumps({"frame_id": row["frame_id"], **result}, ensure_ascii=False) for row, result in zip(rows, results)) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
