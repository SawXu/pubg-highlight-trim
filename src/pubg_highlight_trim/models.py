from __future__ import annotations

from dataclasses import dataclass


@dataclass
class EventDetection:
    duration_sec: float
    event_sec: float | None
    method: str
    detector: str
    text: str = ""
    scores: str = ""
    ocr_seconds: float = 0.0
    sampled_count: int = 0
    detect_seconds: float = 0.0
    ocr_frame_seconds: float = 0.0
    ocr_coarse_frames: int = 0
    ocr_refine_frames: int = 0
    target: str = ""
    event_key: str = ""
    event_count: str = "1"
    event_secs: str = ""
    event_weapon: str = ""
    context_rule: str = ""
    clip_before_sec: float = 0.0
    clip_after_sec: float = 0.0
    ocr_gate_skipped_frames: int = 0
    ocr_requests: int = 0
    ocr_successes: int = 0
    ocr_cache_hits: int = 0
    ocr_skipped: int = 0
    refine_budget_used: int = 0
    assist_budget_used: int = 0
    termination_reason: str = ""
    gate_reason: str = ""
    refine_ocr_seconds: float = 0.0
    assist_ocr_seconds: float = 0.0
