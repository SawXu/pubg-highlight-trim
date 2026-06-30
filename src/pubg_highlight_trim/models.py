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
