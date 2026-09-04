"""Extract auditable OCR ROI frames from local PUBG highlight recordings."""
from __future__ import annotations

import argparse
import csv
import json
import re
from pathlib import Path


def language(text: str) -> str:
    if re.search(r"[A-Za-z]", text) and not re.search(r"[\u4e00-\u9fff]", text):
        return "en"
    if re.search(r"擊|您|淘汰", text):
        return "zh-Hant"
    return "zh-Hans"


def action(text: str, event_keys: str = "") -> str:
    if re.search(r"擊殺|击杀|淘汰|KILLED|ELIMINATED", text, re.I):
        return "eliminate"
    if re.search(r"擊倒|击倒|KNOCKED", text, re.I):
        return "knock"
    if re.search(r"eliminate|淘汰|擊殺|击杀", event_keys, re.I):
        return "eliminate"
    return "unknown"


def event_kind(text: str, event_keys: str = "") -> str:
    if re.search(r"助攻|助殺|ASSIST", text, re.I):
        return "assist"
    if re.search(r"終於|终于|FINALLY", text, re.I):
        return "delayed-elimination"
    return action(text, event_keys)


def find_source(root: Path, name: str) -> Path:
    matches = list(root.rglob(name))
    if not matches:
        raise FileNotFoundError(name)
    originals = [p for p in matches if "original" in {part.lower() for part in p.parts}]
    return originals[0] if originals else matches[0]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--record", action="append", type=Path, required=True)
    parser.add_argument("--media-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limit-per-language", type=int, default=4)
    args = parser.parse_args()

    import cv2

    args.output.mkdir(parents=True, exist_ok=True)
    rows: list[dict[str, object]] = []
    counts = {"zh-Hans": 0, "zh-Hant": 0, "en": 0}
    candidates: list[dict[str, str]] = []
    for record in args.record:
        with record.open(encoding="utf-8-sig", newline="") as handle:
            for item in csv.DictReader(handle):
                if item.get("Status") != "included" or not item.get("PaddleText"):
                    continue
                candidates.append(item)
    selected: list[dict[str, str]] = []
    for lang in counts:
        lang_candidates = [item for item in candidates if language(item["PaddleText"]) == lang]
        # Keep both perspectives, then prioritize assist/delayed-elimination examples.
        for target in ("self-death", "own-kill"):
            target_rows = [item for item in lang_candidates if item.get("Target") == target]
            preferred = sorted(
                target_rows,
                key=lambda item: event_kind(item["PaddleText"], item.get("EventKeys", "")) not in {"assist", "delayed-elimination"},
            )
            selected.extend(preferred[: max(1, args.limit_per_language // 2)])
    for item in selected:
        text = item["PaddleText"]
        lang = language(text)
        try:
            source = find_source(args.media_root, item["Name"])
            event_sec = float(item["EventSec"])
        except (FileNotFoundError, ValueError, KeyError):
            continue
        capture = cv2.VideoCapture(str(source))
        capture.set(cv2.CAP_PROP_POS_MSEC, event_sec * 1000.0)
        ok, frame = capture.read()
        capture.release()
        if not ok or frame is None:
            continue
        height, width = frame.shape[:2]
        x1, y1, x2, y2 = int(width * 0.30), int(height * 0.66), int(width * 0.70), int(height * 0.75)
        roi = frame[y1:y2, x1:x2]
        if roi.size == 0:
            continue
        index = len(rows) + 1
        image_name = f"frame_{index:03d}.jpg"
        cv2.imwrite(str(args.output / image_name), roi, [cv2.IMWRITE_JPEG_QUALITY, 95])
        rows.append({
            "frame_id": index,
            "image": image_name,
            "source": source.name,
            "event_sec": event_sec,
            "language": lang,
            "text": text,
            "action": action(text, item.get("EventKeys", "")),
            "event_kind": event_kind(text, item.get("EventKeys", "")),
            "target": "self-death" if item.get("Target") == "self-death" else "own-kill",
            "boxes": [],
        })
        counts[lang] += 1
    with (args.output / "baseline.jsonl").open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False) + "\n")
    (args.output / "manifest.json").write_text(
        json.dumps({"count": len(rows), "languages": counts, "roi": [0.30, 0.66, 0.70, 0.75]}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps({"count": len(rows), "languages": counts}, ensure_ascii=False))
    return 0 if rows else 1


if __name__ == "__main__":
    raise SystemExit(main())
