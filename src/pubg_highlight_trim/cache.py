from __future__ import annotations

import hashlib
import json
import os
import tempfile
from dataclasses import asdict
from pathlib import Path
from typing import Any

from .models import EventDetection


CACHE_VERSION = "1"
OCR_MODEL_VERSION = "paddleocr-3.7.0"


def content_fingerprint(path: Path, chunk_size: int = 1024 * 1024) -> str:
    digest = hashlib.blake2b(digest_size=20)
    with path.open("rb") as handle:
        while chunk := handle.read(chunk_size):
            digest.update(chunk)
    return digest.hexdigest()


def cache_key(
    path: Path,
    config: Any,
    candidate_times: list[float] | None = None,
    *,
    algorithm_version: str = CACHE_VERSION,
) -> str:
    stat = path.stat()
    payload = {
        "cache_version": CACHE_VERSION,
        "algorithm_version": algorithm_version,
        "model_version": OCR_MODEL_VERSION,
        "content": content_fingerprint(path),
        "size": stat.st_size,
        "target": config.target,
        "language": getattr(config.language, "code", ""),
        "scan": {
            "priority_window": config.priority_window,
            "scan_start": config.scan_start,
            "scan_end": config.scan_end,
            "coarse_step": config.coarse_step,
            "candidate_lookback": config.candidate_lookback,
            "candidate_lookahead": config.candidate_lookahead,
            "candidate_step": config.candidate_step,
            "sampling_mode": config.sampling_mode,
            "adaptive_step": config.adaptive_step,
            "adaptive_window": config.adaptive_window,
            "event_dedupe_seconds": config.event_dedupe_seconds,
            "no_full_scan": config.no_full_scan,
            "candidate_times": sorted(candidate_times or []),
        },
        "roi": config.roi,
        "ocr_width": config.ocr_width,
        "ocr_min_interval": config.ocr_min_interval,
        "ocr_max_calls": config.ocr_max_calls,
        "gate": {
            "enabled": config.brightness_gate,
            "mode": config.brightness_gate_mode,
            "roi": config.brightness_gate_roi,
            "width": config.brightness_gate_width,
        },
        "refine": {
            "before": config.refine_before,
            "after": config.refine_after,
            "step": config.refine_step,
            "search_step": config.refine_search_step,
            "max_frames": config.refine_max_frames,
            "max_seconds": config.refine_max_seconds,
        },
        "assist": {
            "roi": config.assist_roi,
            "after": config.assist_after,
            "step": config.assist_step,
            "max_frames": config.assist_max_frames,
            "max_seconds": config.assist_max_seconds,
        },
    }
    encoded = json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.blake2b(encoded, digest_size=20).hexdigest()


def _paths(cache_dir: Path, key: str) -> tuple[Path, Path]:
    return cache_dir / f"{key}.json", cache_dir / f"{key}.complete"


def load_detection_cache(cache_dir: Path | None, key: str) -> list[EventDetection] | None:
    if cache_dir is None:
        return None
    data_path, complete_path = _paths(cache_dir, key)
    if not data_path.is_file() or not complete_path.is_file():
        return None
    try:
        if complete_path.read_text(encoding="ascii").strip() != CACHE_VERSION:
            return None
        payload = json.loads(data_path.read_text(encoding="utf-8"))
        if payload.get("cache_version") != CACHE_VERSION:
            return None
        return [EventDetection(**item) for item in payload.get("detections", [])]
    except (OSError, TypeError, ValueError, KeyError):
        return None


def save_detection_cache(cache_dir: Path | None, key: str, detections: list[EventDetection]) -> None:
    if cache_dir is None:
        return
    cache_dir.mkdir(parents=True, exist_ok=True)
    data_path, complete_path = _paths(cache_dir, key)
    payload = {
        "cache_version": CACHE_VERSION,
        "detections": [asdict(detection) for detection in detections],
    }
    fd, temp_name = tempfile.mkstemp(prefix=f"{key}.", suffix=".tmp", dir=cache_dir)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as handle:
            json.dump(payload, handle, ensure_ascii=False, sort_keys=True)
            handle.flush()
            os.fsync(handle.fileno())
        complete_path.unlink(missing_ok=True)
        os.replace(temp_name, data_path)
        complete_path.write_text(CACHE_VERSION, encoding="ascii")
    finally:
        if os.path.exists(temp_name):
            os.unlink(temp_name)
