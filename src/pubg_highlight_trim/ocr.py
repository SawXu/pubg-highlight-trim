from __future__ import annotations

import os
import re
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .models import EventDetection
from .runtime import first_existing_runtime_path

os.environ.setdefault("PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK", "True")

SELF_STRICT_RE = re.compile(r"(击倒了你|淘汰了你)")
SELF_ZONE_DOWNED_RE = re.compile(r"(你在安全区外倒地了|安全区外倒地了|安全区外倒地)")
SELF_FUZZY_RE = re.compile(r"(击倒.{0,2}你|淘.{0,2}了?你|倒了你)")


class OcrUnavailable(RuntimeError):
    pass


def configure_paddlex_cache() -> None:
    if os.environ.get("PADDLE_PDX_CACHE_HOME"):
        return
    bundled_cache = first_existing_runtime_path("vendor", "paddlex_cache", "official_models")
    if bundled_cache:
        os.environ["PADDLE_PDX_CACHE_HOME"] = str(bundled_cache.parent)


@dataclass
class OcrResult:
    text: str
    scores: str
    seconds: float
    method: str


@dataclass
class OcrConfig:
    priority_window: list[tuple[float, float]] = field(default_factory=lambda: [(28.0, 42.0), (44.0, 52.0)])
    scan_start: float = 0.0
    scan_end: float | None = None
    coarse_step: float = 3.0
    candidate_lookback: float = 8.0
    candidate_lookahead: float = 0.5
    candidate_step: float = 3.0
    refine_before: float = 6.0
    refine_after: float = 0.4
    refine_step: float = 0.1
    no_full_scan: bool = False
    roi: tuple[float, float, float, float] = (0.22, 0.62, 0.78, 0.84)
    ocr_width: int = 1152


def normalize_text(text: str) -> str:
    return re.sub(r"\s+", "", text or "")


def classify_self_text(text: str) -> str | None:
    text = normalize_text(text)
    if SELF_STRICT_RE.search(text):
        return "paddle-strict-self-text"
    if SELF_ZONE_DOWNED_RE.search(text):
        return "paddle-zone-self-downed-text"
    if SELF_FUZZY_RE.search(text):
        return "paddle-fuzzy-self-text"
    return None


def load_backend() -> tuple[Any, Any]:
    configure_paddlex_cache()
    try:
        import cv2  # type: ignore
        from paddleocr import PaddleOCR  # type: ignore
    except Exception as exc:  # pragma: no cover - depends on optional packages.
        raise OcrUnavailable(
            "OCR dependencies are missing. Install with: python -m pip install -e .[ocr]"
        ) from exc
    started = time.time()
    ocr = PaddleOCR(lang="ch", use_doc_orientation_classify=False, use_doc_unwarping=False, use_textline_orientation=False)
    print(f"PaddleOCR initialized in {time.time() - started:.1f}s", flush=True)
    return cv2, ocr


def time_range(start: float, end: float, step: float) -> list[float]:
    if step <= 0:
        raise ValueError("step must be positive")
    out: list[float] = []
    t = start
    while t <= end + 1e-6:
        out.append(round(t, 3))
        t += step
    return out


def build_text_priority_scan_times(duration: float, candidate_times: list[float], config: OcrConfig) -> list[float]:
    end_limit = min(duration, config.scan_end if config.scan_end is not None else duration)
    times: set[int] = set()

    if not config.no_full_scan:
        times.update(int(round(t * 1000)) for t in time_range(config.scan_start, end_limit, config.coarse_step))

    for start, stop in config.priority_window:
        lo = max(config.scan_start, 0.0, start)
        hi = min(end_limit, stop)
        if hi >= lo:
            times.update(int(round(t * 1000)) for t in time_range(lo, hi, config.candidate_step))

    for candidate in candidate_times:
        lo = max(config.scan_start, 0.0, candidate - config.candidate_lookback)
        hi = min(end_limit, candidate + config.candidate_lookahead)
        if hi >= lo:
            times.update(int(round(t * 1000)) for t in time_range(lo, hi, config.candidate_step))

    return [key / 1000 for key in sorted(times)]


def ocr_at(cv2_module: Any, cap: Any, ocr: Any, sec: float, config: OcrConfig) -> OcrResult:
    cap.set(cv2_module.CAP_PROP_POS_MSEC, max(0.0, sec) * 1000)
    ok, frame = cap.read()
    if not ok or frame is None:
        return OcrResult("", "", 0.0, "frame-read-failed")

    h, w = frame.shape[:2]
    x1, y1, x2, y2 = config.roi
    crop = frame[int(h * y1) : int(h * y2), int(w * x1) : int(w * x2)]
    if config.ocr_width > 0 and crop.shape[1] > config.ocr_width:
        scale = config.ocr_width / crop.shape[1]
        crop = cv2_module.resize(crop, (config.ocr_width, max(1, int(crop.shape[0] * scale))), interpolation=cv2_module.INTER_AREA)

    started = time.time()
    raw = ocr.predict(crop)
    elapsed = time.time() - started

    texts: list[str] = []
    scores: list[str] = []
    for item in raw:
        data = item.json.get("res", {}) if hasattr(item, "json") else item
        texts.extend(data.get("rec_texts", []) or [])
        for score in data.get("rec_scores", []) or []:
            try:
                scores.append(f"{float(score):.3f}")
            except Exception:
                scores.append(str(score))
    text = normalize_text("".join(texts))
    method = classify_self_text(text) or "paddle-not-self-text"
    return OcrResult(text, ";".join(scores), elapsed, method)


def refine_event(cv2_module: Any, cap: Any, ocr: Any, coarse_sec: float, duration: float, config: OcrConfig) -> tuple[float, OcrResult, int]:
    lo = max(0.0, coarse_sec - config.refine_before)
    hi = min(duration, coarse_sec + config.refine_after)
    sampled = 0
    best: tuple[float, OcrResult] | None = None

    rough_step = max(config.refine_step, 0.5)
    rough_hit: tuple[float, OcrResult] | None = None
    for t in time_range(lo, hi, rough_step):
        sampled += 1
        result = ocr_at(cv2_module, cap, ocr, t, config)
        if classify_self_text(result.text):
            rough_hit = (t, result)
            break
        if result.text and best is None:
            best = (t, result)
    if rough_hit:
        fine_lo = max(lo, rough_hit[0] - rough_step)
        fine_hi = min(hi, rough_hit[0] + config.refine_step)
        for t in time_range(fine_lo, fine_hi, config.refine_step):
            sampled += 1
            result = ocr_at(cv2_module, cap, ocr, t, config)
            if classify_self_text(result.text):
                return t, result, sampled
            if result.text and best is None:
                best = (t, result)
        return rough_hit[0], rough_hit[1], sampled
    if best:
        return best[0], best[1], sampled
    return coarse_sec, OcrResult("", "", 0.0, "paddle-refine-missed"), sampled


def detect_event(path: Path, cv2_module: Any, ocr: Any, duration: float, candidate_times: list[float], config: OcrConfig) -> EventDetection:
    cap = cv2_module.VideoCapture(str(path))
    sampled_count = 0
    total_ocr_seconds = 0.0
    last_text = ""
    last_scores = ""
    last_method = "not-scanned"
    try:
        for sec in build_text_priority_scan_times(duration, candidate_times, config):
            sampled_count += 1
            result = ocr_at(cv2_module, cap, ocr, sec, config)
            total_ocr_seconds += result.seconds
            if result.text:
                last_text, last_scores, last_method = result.text, result.scores, result.method
            if classify_self_text(result.text):
                event_sec, refined, refined_count = refine_event(cv2_module, cap, ocr, sec, duration, config)
                sampled_count += refined_count
                total_ocr_seconds += refined.seconds
                method = classify_self_text(refined.text) or result.method
                return EventDetection(
                    duration,
                    event_sec,
                    method,
                    "ocr",
                    refined.text or result.text,
                    refined.scores or result.scores,
                    total_ocr_seconds,
                    sampled_count,
                )
    finally:
        cap.release()

    return EventDetection(
        duration,
        None,
        last_method if last_method != "not-scanned" else "paddle-no-self-text-found",
        "ocr",
        last_text,
        last_scores,
        total_ocr_seconds,
        sampled_count,
    )
