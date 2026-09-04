"""Compare frame-level OCR JSONL outputs from Python and Rust engines."""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def normalize(text: str) -> str:
    return re.sub(r"\s+", "", text or "")


def load(path: Path) -> dict[str, dict]:
    rows = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        rows[str(row["frame_id"])] = row
    return rows


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("python_jsonl", type=Path)
    parser.add_argument("rust_jsonl", type=Path)
    parser.add_argument("--allow-missing-boxes", action="store_true")
    args = parser.parse_args()
    python_rows = load(args.python_jsonl)
    rust_rows = load(args.rust_jsonl)
    frame_ids = sorted(set(python_rows) | set(rust_rows))
    text_matches = action_matches = event_kind_matches = target_matches = box_matches = 0
    comparable_boxes = 0
    timing_deltas: list[float] = []
    for frame_id in frame_ids:
        left, right = python_rows.get(frame_id, {}), rust_rows.get(frame_id, {})
        text_matches += normalize(left.get("text", "")) == normalize(right.get("text", ""))
        action_matches += left.get("action") == right.get("action")
        event_kind_matches += left.get("event_kind") == right.get("event_kind")
        target_matches += left.get("target") == right.get("target")
        left_boxes, right_boxes = left.get("boxes"), right.get("boxes")
        if left_boxes is not None and right_boxes is not None and (left_boxes or right_boxes or not args.allow_missing_boxes):
            comparable_boxes += 1
            box_matches += left_boxes == right_boxes
        if "inference_ms" in left and "inference_ms" in right:
            timing_deltas.append(abs(float(left["inference_ms"]) - float(right["inference_ms"])))
    total = len(frame_ids) or 1
    report = {
        "frames": len(frame_ids),
        "text_match_rate": round(text_matches / total, 6),
        "action_match_rate": round(action_matches / total, 6),
        "event_kind_match_rate": round(event_kind_matches / total, 6),
        "target_match_rate": round(target_matches / total, 6),
        "box_order_match_rate": round(box_matches / comparable_boxes, 6) if comparable_boxes else None,
        "box_comparable_frames": comparable_boxes,
        "mean_inference_delta_ms": round(sum(timing_deltas) / len(timing_deltas), 6) if timing_deltas else None,
        "missing_from_python": sorted(set(rust_rows) - set(python_rows)),
        "missing_from_rust": sorted(set(python_rows) - set(rust_rows)),
    }
    print(json.dumps(report, ensure_ascii=False, sort_keys=True))
    return 0 if (
        not report["missing_from_python"]
        and not report["missing_from_rust"]
        and text_matches == len(frame_ids)
        and action_matches == len(frame_ids)
        and event_kind_matches == len(frame_ids)
        and target_matches == len(frame_ids)
        and (args.allow_missing_boxes or (comparable_boxes == len(frame_ids) and box_matches == len(frame_ids)))
    ) else 1


if __name__ == "__main__":
    raise SystemExit(main())
