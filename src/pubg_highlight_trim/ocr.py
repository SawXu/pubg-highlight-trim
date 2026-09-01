from __future__ import annotations

import os
import re
import time
from bisect import insort
from dataclasses import dataclass, field, replace
from difflib import SequenceMatcher
from math import isfinite
from pathlib import Path
from typing import Any, Callable

from .game_languages import GameLanguageProfile, default_game_language_profile
from .models import EventDetection
from .runtime import first_existing_runtime_path

os.environ.setdefault("PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK", "True")


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
    frame_seconds: float = 0.0
    cache_hit: bool = False
    gate_reason: str = ""
    skipped_reason: str = ""


@dataclass
class OcrRuntimeStats:
    """Counters for one source scan; kept separate from immutable event data."""

    ocr_requests: int = 0
    ocr_successes: int = 0
    ocr_cache_hits: int = 0
    ocr_skipped: int = 0
    gate_skipped: int = 0
    refine_budget_used: int = 0
    assist_budget_used: int = 0
    refine_calls: int = 0
    assist_calls: int = 0
    refine_ocr_seconds: float = 0.0
    assist_ocr_seconds: float = 0.0
    last_gate_reason: str = ""
    gate_reasons: list[str] = field(default_factory=list)
    last_skip_reason: str = ""
    last_cache_hit: bool = False
    last_request_sec: float | None = None
    termination_reason: str = ""


@dataclass(frozen=True)
class TextEvent:
    target: str
    method: str
    action: str
    subject: str
    key: str
    text: str
    weapon: str = ""


@dataclass
class OcrConfig:
    target: str = "self-death"
    language: GameLanguageProfile = field(default_factory=default_game_language_profile)
    priority_window: list[tuple[float, float]] = field(default_factory=lambda: [(31.0, 43.0), (45.0, 53.0)])
    scan_start: float = 0.0
    scan_end: float | None = None
    coarse_step: float = 4.0
    candidate_lookback: float = 8.0
    candidate_lookahead: float = 0.5
    candidate_step: float = 4.0
    refine_before: float = 6.0
    refine_after: float = 0.4
    refine_step: float = 0.5
    refine_search_step: float = 2.0
    event_dedupe_seconds: float = 5.0
    no_full_scan: bool = False
    roi: tuple[float, float, float, float] = (0.30, 0.66, 0.70, 0.75)
    assist_roi: tuple[float, float, float, float] = (0.25, 0.64, 0.75, 0.86)
    assist_after: float = 1.2
    assist_step: float = 0.5
    ocr_width: int = 768
    brightness_gate: bool = False
    brightness_gate_mode: str = "full"
    brightness_gate_roi: tuple[float, float, float, float] = (0.26, 0.635, 0.74, 0.725)
    brightness_gate_width: int = 768
    sampling_mode: str = "fixed"
    adaptive_step: float = 0.5
    adaptive_window: float = 2.0
    ocr_max_calls: int | None = None
    ocr_min_interval: float = 0.0
    refine_max_frames: int | None = None
    assist_max_frames: int | None = None
    refine_max_seconds: float | None = None
    assist_max_seconds: float | None = None
    frame_cache: dict[tuple[str, int, tuple[float, float, float, float], int], tuple[str, str]] = field(
        default_factory=dict,
        repr=False,
    )
    decoded_frame_cache: dict[int, Any] = field(default_factory=dict, repr=False)
    decoded_frame_cache_size: int = 8
    runtime: OcrRuntimeStats = field(default_factory=OcrRuntimeStats, repr=False)

    def __post_init__(self) -> None:
        for name, roi in (("roi", self.roi), ("assist_roi", self.assist_roi), ("brightness_gate_roi", self.brightness_gate_roi)):
            validate_roi(roi, name)
        if self.brightness_gate_mode not in {"full", "light", "none"}:
            raise ValueError("brightness_gate_mode must be full, light, or none")
        if self.sampling_mode not in {"fixed", "adaptive"}:
            raise ValueError("sampling_mode must be fixed or adaptive")
        if self.scan_start < 0 or not isfinite(self.scan_start):
            raise ValueError("scan_start must be a finite non-negative number")
        if self.scan_end is not None and (self.scan_end <= self.scan_start or not isfinite(self.scan_end)):
            raise ValueError("scan_end must be greater than scan_start")
        for start, stop in self.priority_window:
            if start < 0 or stop <= start or not isfinite(start) or not isfinite(stop):
                raise ValueError("priority windows must satisfy 0 <= start < end")
        for name in ("coarse_step", "candidate_step", "refine_step", "refine_search_step", "assist_step", "adaptive_step"):
            if getattr(self, name) <= 0:
                raise ValueError(f"{name} must be positive")
        for name in ("candidate_lookback", "candidate_lookahead", "refine_before", "refine_after", "assist_after", "adaptive_window", "ocr_min_interval"):
            value = getattr(self, name)
            if value < 0 or not isfinite(value):
                raise ValueError(f"{name} must be a finite non-negative number")
        for name in ("ocr_max_calls", "refine_max_frames", "assist_max_frames"):
            value = getattr(self, name)
            if value is not None and value < 0:
                raise ValueError(f"{name} must be non-negative or None")
        for name in ("refine_max_seconds", "assist_max_seconds"):
            value = getattr(self, name)
            if value is not None and (value < 0 or not isfinite(value)):
                raise ValueError(f"{name} must be finite and non-negative or None")
        if self.ocr_width < 0 or self.brightness_gate_width < 0:
            raise ValueError("OCR widths must be non-negative")
        if self.decoded_frame_cache_size < 1:
            raise ValueError("decoded_frame_cache_size must be positive")

    def reset_runtime(self) -> None:
        self.runtime = OcrRuntimeStats()


def validate_roi(roi: tuple[float, float, float, float], name: str = "ROI") -> tuple[float, float, float, float]:
    try:
        size = len(roi)
    except TypeError as exc:
        raise ValueError(f"{name} must contain four values") from exc
    if size != 4:
        raise ValueError(f"{name} must contain four values")
    x1, y1, x2, y2 = roi
    try:
        finite = all(isfinite(value) for value in roi)
    except TypeError as exc:
        raise ValueError(f"{name} values must be numeric") from exc
    if not finite:
        raise ValueError(f"{name} values must be finite")
    if not (0 <= x1 < x2 <= 1 and 0 <= y1 < y2 <= 1):
        raise ValueError(f"{name} values must be ratios in ascending order, between 0 and 1")
    return roi


def normalize_text(text: str) -> str:
    return re.sub(r"\s+", "", text or "")


def _language(profile: GameLanguageProfile | None = None) -> GameLanguageProfile:
    return profile or default_game_language_profile()


def classify_self_text(text: str, profile: GameLanguageProfile | None = None) -> str | None:
    profile = _language(profile)
    text = normalize_text(text)
    if profile.self_strict_re.search(text):
        return "paddle-strict-self-text"
    if profile.self_zone_downed_re.search(text):
        return "paddle-zone-self-downed-text"
    if profile.self_fuzzy_re.search(text):
        return "paddle-fuzzy-self-text"
    return None


def _clean_subject(text: str, profile: GameLanguageProfile | None = None) -> str:
    profile = _language(profile)
    text = normalize_text(text)
    text = profile.own_kill_victim_stop_re.split(text, maxsplit=1)[0]
    text = text.strip("，。,.、:：;；|/\\()（）[]【】 ")
    text = profile.subject_suffix_noise_re.sub("", text)
    text = text.strip("，。,.、:：;；|/\\()（）[]【】 ")
    return text[:48]


def _subject_key(text: str, profile: GameLanguageProfile | None = None) -> str:
    cleaned = _clean_subject(text, profile)
    key = "".join(ch.lower() for ch in cleaned if ch.isalnum() or ch in "_-[]")
    return key or normalize_text(cleaned).lower()


def _clean_weapon(text: str, profile: GameLanguageProfile | None = None) -> str:
    profile = _language(profile)
    text = normalize_text(text)
    text = profile.weapon_suffix_noise_re.sub("", text)
    text = text.strip("，。,.、:：;；|/\\()（）[]【】 ")
    return text[:32]


def _own_kill_weapon(prefix: str, profile: GameLanguageProfile | None = None) -> str:
    profile = _language(profile)
    match = profile.weapon_prefix_re.search(normalize_text(prefix))
    return _clean_weapon(match.group("weapon"), profile) if match else ""


def _self_event_weapon(actor_text: str, profile: GameLanguageProfile | None = None) -> str:
    profile = _language(profile)
    match = profile.self_weapon_re.search(normalize_text(actor_text))
    return _clean_weapon(match.group("weapon"), profile) if match else ""


def _self_actor_subject(actor_text: str, profile: GameLanguageProfile | None = None) -> str:
    profile = _language(profile)
    text = normalize_text(actor_text)
    weapon_match = profile.self_weapon_re.search(text)
    if weapon_match:
        text = text[: weapon_match.start()]
    text = text.strip("，。,.、:：;；|/\\()（）[]【】 ")
    return text[:48]


def _match_action_text(match: re.Match[str] | None) -> str:
    if match is None:
        return ""
    action = match.groupdict().get("action")
    if action:
        return action
    try:
        return match.group(1)
    except IndexError:
        return match.group(0)


def is_molotov_weapon(weapon: str, profile: GameLanguageProfile | None = None) -> bool:
    profile = _language(profile)
    return bool(profile.molotov_weapon_re.search(normalize_text(weapon)))


def extract_own_kill_events(
    text: str,
    allow_assist: bool = False,
    profile: GameLanguageProfile | None = None,
) -> list[TextEvent]:
    profile = _language(profile)
    text = normalize_text(text)
    if not text:
        return []
    events: list[TextEvent] = []
    for match in profile.own_kill_event_re.finditer(text):
        raw = normalize_text(match.group(0))
        if is_delayed_own_elim_text(raw, profile):
            continue
        assist_probe = text[match.start() : min(len(text), match.end() + 24)]
        if not allow_assist and (
            is_assist_own_kill_text(raw, profile) or is_assist_own_kill_text(assist_probe, profile)
        ):
            continue
        subject = _clean_subject(match.group("victim"), profile)
        subject_key = _subject_key(subject, profile)
        if not subject_key or subject_key in profile.self_subject_keys:
            continue
        action = profile.canonical_action(match.group("action"))
        prefix_end = match.start("action") - match.start()
        weapon = _own_kill_weapon(raw[:prefix_end], profile)
        if not weapon and "weapon" in match.groupdict():
            weapon = _clean_weapon(match.group("weapon") or "", profile)
        events.append(
            TextEvent(
                "own-kill",
                "paddle-own-kill-text",
                action,
                subject,
                f"own-kill:{action}:{subject_key}",
                raw,
                weapon,
            )
        )
    return events


def extract_self_events(text: str, profile: GameLanguageProfile | None = None) -> list[TextEvent]:
    profile = _language(profile)
    text = normalize_text(text)
    method = classify_self_text(text, profile)
    if not method:
        return []
    if profile.self_zone_downed_re.search(text):
        return [TextEvent("self-death", method, "zone-downed", "zone", "self-death:zone", text)]
    action_match = profile.self_strict_re.search(text) or profile.self_fuzzy_re.search(text)
    action = profile.canonical_action(_match_action_text(action_match)) if action_match else "self-death"
    actor = text[: action_match.start()] if action_match else text
    actor_key = _subject_key(actor[-32:], profile) or "unknown"
    weapon = _self_event_weapon(actor, profile)
    if not weapon and action_match and "weapon" in action_match.groupdict():
        weapon = _clean_weapon(action_match.group("weapon") or "", profile)
    return [TextEvent("self-death", method, action, actor, f"self-death:{action}:{actor_key}", text, weapon)]


def extract_text_events(text: str, target: str, profile: GameLanguageProfile | None = None) -> list[TextEvent]:
    profile = _language(profile)
    events: list[TextEvent] = []
    if target in {"self-death", "both"}:
        events.extend(extract_self_events(text, profile))
    if target in {"own-kill", "both"}:
        events.extend(extract_own_kill_events(text, profile=profile))
    return events


def _split_event_key(event: TextEvent) -> tuple[str, str, str]:
    parts = event.key.split(":", 2)
    if len(parts) == 3:
        return parts[0], parts[1], parts[2]
    return event.target, event.action, event.subject


def same_text_event(left: TextEvent, right: TextEvent) -> bool:
    if left.target != right.target or left.action != right.action:
        return False
    if left.key == right.key:
        return True
    _, _, left_subject = _split_event_key(left)
    _, _, right_subject = _split_event_key(right)
    return _same_stable_subject(left_subject, right_subject)


def _same_stable_subject(left_subject: str, right_subject: str) -> bool:
    if not left_subject or not right_subject:
        return False
    length_ratio = min(len(left_subject), len(right_subject)) / max(len(left_subject), len(right_subject))
    if (
        length_ratio >= 0.75
        and len(left_subject) >= 4
        and len(right_subject) >= 4
        and (left_subject in right_subject or right_subject in left_subject)
    ):
        return True
    return length_ratio >= 0.75 and SequenceMatcher(None, left_subject, right_subject).ratio() >= 0.86


def _same_noisy_subject(left_subject: str, right_subject: str, target: str) -> bool:
    if _same_stable_subject(left_subject, right_subject):
        return True
    if not left_subject or not right_subject:
        return False
    if target == "self-death":
        left_anchor = _self_death_actor_anchor(left_subject)
        right_anchor = _self_death_actor_anchor(right_subject)
        if left_anchor and left_anchor == right_anchor:
            return True
    left_key = _compact_latin_subject_key(left_subject)
    right_key = _compact_latin_subject_key(right_subject)
    if not left_key or not right_key:
        return False
    shorter, longer = sorted([left_key, right_key], key=len)
    # Paddle occasionally appends HUD/texture text to an otherwise stable ID on
    # adjacent frames, for example ASKKZM -> ASKKZMPNC2020.
    if len(shorter) >= 5 and shorter in longer:
        return True
    common = 0
    for left_char, right_char in zip(left_key, right_key):
        if left_char != right_char:
            break
        common += 1
    has_digit = any(ch.isdigit() for ch in left_key + right_key)
    return has_digit and common >= 6 and common / len(shorter) >= 0.75


def same_noisy_close_event(left: TextEvent, right: TextEvent) -> bool:
    if same_text_event(left, right):
        return True
    if left.target != right.target or left.action != right.action:
        return False
    _, _, left_subject = _split_event_key(left)
    _, _, right_subject = _split_event_key(right)
    return _same_noisy_subject(left_subject, right_subject, left.target)


def _followup_subject(event: TextEvent, profile: GameLanguageProfile | None = None) -> str:
    if event.target == "self-death":
        subject = _self_actor_subject(event.subject, profile)
        return _subject_key(subject, profile)
    _, _, subject = _split_event_key(event)
    return subject


def _same_followup_subject(
    left: TextEvent,
    right: TextEvent,
    profile: GameLanguageProfile | None = None,
) -> bool:
    if left.target != right.target:
        return False
    return _same_noisy_subject(
        _followup_subject(left, profile),
        _followup_subject(right, profile),
        left.target,
    )


def _is_prior_event(seen_sec: float, event_sec: float | None) -> bool:
    return event_sec is None or seen_sec <= event_sec + 1e-6


def _followup_elimination_skip_method(
    event: TextEvent,
    seen_events: list[tuple[TextEvent, float]],
    profile: GameLanguageProfile | None = None,
    event_sec: float | None = None,
) -> str | None:
    if event.action != "eliminate":
        return None
    if event.target == "self-death" and any(
        seen.target == "self-death" and seen.action == "knock" and _is_prior_event(seen_sec, event_sec)
        for seen, seen_sec in seen_events
    ):
        return "paddle-followup-self-elimination-skipped-after-knock"
    if any(
        seen.action == "knock"
        and _is_prior_event(seen_sec, event_sec)
        and _same_followup_subject(event, seen, profile)
        for seen, seen_sec in seen_events
    ):
        return "paddle-followup-elimination-skipped"
    return None


def _self_death_actor_anchor(subject: str) -> str:
    ascii_key = _latin_subject_key(subject)
    match = re.search(r"\[[a-z0-9]+\][a-z0-9_-]{3,}", ascii_key)
    return match.group(0) if match else ""


def _latin_subject_key(subject: str) -> str:
    return "".join(ch.lower() for ch in subject if ch.isascii() and (ch.isalnum() or ch in "_-[]"))


def _compact_latin_subject_key(subject: str) -> str:
    return "".join(ch.lower() for ch in subject if ch.isascii() and ch.isalnum())


def classify_own_kill_text(text: str, profile: GameLanguageProfile | None = None) -> str | None:
    events = extract_own_kill_events(text, profile=profile)
    return events[0].method if events else None


def has_assist_text(text: str, profile: GameLanguageProfile | None = None) -> bool:
    profile = _language(profile)
    return bool(profile.own_kill_assist_re.search(normalize_text(text)))


def is_delayed_own_elim_text(text: str, profile: GameLanguageProfile | None = None) -> bool:
    profile = _language(profile)
    return bool(profile.own_kill_delayed_elim_re.search(normalize_text(text)))


def is_assist_own_kill_text(text: str, profile: GameLanguageProfile | None = None) -> bool:
    profile = _language(profile)
    text = normalize_text(text)
    assist = profile.own_kill_assist_re.search(text)
    if not assist:
        return False
    if profile.own_kill_scoreboard_assist_re.search(text):
        return False
    kill_count = profile.own_kill_count_re.search(text)
    return not kill_count or assist.start() < kill_count.start()


def has_own_kill_candidate(text: str, profile: GameLanguageProfile | None = None) -> bool:
    profile = _language(profile)
    return bool(profile.own_kill_strict_re.search(normalize_text(text)))


def classify_target_kind(text: str, target: str, profile: GameLanguageProfile | None = None) -> tuple[str, str] | None:
    events = extract_text_events(text, target, profile)
    return (events[0].method, events[0].target) if events else None


def classify_target_text(text: str, target: str, profile: GameLanguageProfile | None = None) -> str | None:
    classified = classify_target_kind(text, target, profile)
    return classified[0] if classified else None


def load_backend(profile: GameLanguageProfile | None = None, *, verbose: bool = False) -> tuple[Any, Any]:
    profile = _language(profile)
    configure_paddlex_cache()
    try:
        import cv2  # type: ignore
        from paddleocr import PaddleOCR  # type: ignore
    except Exception as exc:  # pragma: no cover - depends on optional packages.
        raise OcrUnavailable(
            "OCR dependencies are missing. Install with: python -m pip install -e .[ocr]"
        ) from exc
    started = time.time()
    ocr = PaddleOCR(
        lang=profile.paddle_lang,
        use_doc_orientation_classify=False,
        use_doc_unwarping=False,
        use_textline_orientation=False,
    )
    if verbose:
        print(f"PaddleOCR initialized lang={profile.paddle_lang} in {time.time() - started:.1f}s", flush=True)
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


def build_adaptive_scan_times(duration: float, candidate_times: list[float], config: OcrConfig) -> list[float]:
    """Build coarse discovery samples and exact candidate-time anchors.

    Dense samples are added later only after a seed sample identifies a target
    event. Keeping the initial schedule coarse prevents the adaptive path from
    paying the dense OCR cost across every priority window.
    """
    end_limit = min(duration, config.scan_end if config.scan_end is not None else duration)
    times: set[int] = set()
    if not config.no_full_scan:
        times.update(int(round(t * 1000)) for t in time_range(config.scan_start, end_limit, config.coarse_step))
    for start, stop in config.priority_window:
        lo = max(config.scan_start, 0.0, start)
        hi = min(end_limit, stop)
        if hi >= lo:
            times.update(int(round(t * 1000)) for t in time_range(lo, hi, config.candidate_step))
            times.add(int(round(lo * 1000)))
            times.add(int(round(hi * 1000)))
    for candidate in candidate_times:
        lo = max(config.scan_start, 0.0, candidate - config.candidate_lookback)
        hi = min(end_limit, candidate + config.candidate_lookahead)
        if hi >= lo:
            times.update(int(round(t * 1000)) for t in time_range(lo, hi, config.candidate_step))
            times.add(int(round(lo * 1000)))
            times.add(int(round(hi * 1000)))
            if lo <= candidate <= hi:
                times.add(int(round(candidate * 1000)))
    return [key / 1000 for key in sorted(times)]


def _crop_frame(cv2_module: Any, frame: Any, roi: tuple[float, float, float, float], ocr_width: int) -> Any:
    h, w = frame.shape[:2]
    x1, y1, x2, y2 = roi
    crop = frame[int(h * y1) : int(h * y2), int(w * x1) : int(w * x2)]
    if ocr_width > 0 and crop.shape[1] > ocr_width:
        scale = ocr_width / crop.shape[1]
        crop = cv2_module.resize(crop, (ocr_width, max(1, int(crop.shape[0] * scale))), interpolation=cv2_module.INTER_AREA)
    return crop


def brightness_gate_result(cv2_module: Any, frame: Any, config: OcrConfig) -> tuple[bool, str]:
    if config.brightness_gate_mode == "none":
        return True, "disabled"
    import numpy as np

    crop = _crop_frame(cv2_module, frame, config.brightness_gate_roi, 0)
    if crop.size == 0:
        return True, "empty-roi"

    gate_width = max(1, config.brightness_gate_width)
    scale = gate_width / crop.shape[1]
    crop = cv2_module.resize(
        crop,
        (gate_width, max(1, int(round(crop.shape[0] * scale)))),
        interpolation=cv2_module.INTER_AREA if scale < 1 else cv2_module.INTER_LINEAR,
    )
    if config.brightness_gate_mode == "light":
        gray = cv2_module.cvtColor(crop, cv2_module.COLOR_BGR2GRAY)
        bright_pixels = int((gray >= 155).sum())
        threshold = max(8, int(crop.shape[0] * crop.shape[1] * 0.002))
        return (True, "light-brightness-passed") if bright_pixels >= threshold else (False, "light-low-brightness")

    hsv = cv2_module.cvtColor(crop, cv2_module.COLOR_BGR2HSV)
    gray = cv2_module.cvtColor(crop, cv2_module.COLOR_BGR2GRAY)
    white = cv2_module.inRange(hsv, (0, 0, 155), (179, 105, 255))
    yellow = cv2_module.inRange(hsv, (3, 70, 135), (42, 255, 255))
    bright = cv2_module.bitwise_or(white, yellow)
    dark = cv2_module.inRange(gray, 0, 100)
    dark_near = cv2_module.dilate(dark, np.ones((5, 5), dtype=np.uint8))
    outlined = cv2_module.bitwise_and(bright, dark_near)
    neighbour_count = cv2_module.boxFilter(
        (outlined > 0).astype(np.uint8),
        cv2_module.CV_16U,
        (7, 5),
        normalize=False,
    )
    grouped = ((outlined > 0) & (neighbour_count >= 3)).astype(np.uint8)
    if int(grouped.sum()) < 1450:
        return False, "insufficient-bright-outline"

    row_counts = grouped.sum(axis=1)
    if int(np.convolve(row_counts, np.ones(11, dtype=np.int32), mode="same").max()) < 1000:
        return False, "insufficient-horizontal-structure"

    closed = cv2_module.morphologyEx(
        grouped,
        cv2_module.MORPH_CLOSE,
        np.ones((3, 11), dtype=np.uint8),
    )
    _, _, stats, _ = cv2_module.connectedComponentsWithStats(closed, 8)
    max_height = max(3, int(crop.shape[0] * 0.6))
    passed = any(3 <= height <= max_height and width >= 20 for _, _, width, height, _ in stats[1:])
    return (True, "passed") if passed else (False, "no-connected-text")


def has_bright_event_text(cv2_module: Any, frame: Any, config: OcrConfig) -> bool:
    return brightness_gate_result(cv2_module, frame, config)[0]


def _predict_text(ocr: Any, crop: Any) -> tuple[str, str, float]:
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
    return normalize_text("".join(texts)), ";".join(scores), elapsed


def _ocr_crop_at(
    cv2_module: Any,
    cap: Any,
    ocr: Any,
    sec: float,
    config: OcrConfig,
    roi: tuple[float, float, float, float],
    cache_namespace: str,
    apply_brightness_gate: bool = False,
) -> tuple[str, str, float, float, bool, bool]:
    config.runtime.last_skip_reason = ""
    config.runtime.last_gate_reason = ""
    config.runtime.last_cache_hit = False
    frame_key = int(round(max(0.0, sec) * 1000))
    cache_key = (cache_namespace, frame_key, roi, config.ocr_width)
    cached = config.frame_cache.get(cache_key)
    if cached is not None:
        config.runtime.ocr_cache_hits += 1
        config.runtime.last_cache_hit = True
        return cached[0], cached[1], 0.0, 0.0, True, False

    if config.ocr_max_calls is not None and config.runtime.ocr_requests >= config.ocr_max_calls:
        config.runtime.ocr_skipped += 1
        config.runtime.last_skip_reason = "ocr-budget-exhausted"
        config.runtime.termination_reason = "ocr-budget-exhausted"
        return "", "", 0.0, 0.0, True, False
    if (
        config.ocr_min_interval > 0
        and config.runtime.last_request_sec is not None
        and abs(sec - config.runtime.last_request_sec) < config.ocr_min_interval - 1e-6
    ):
        config.runtime.ocr_skipped += 1
        config.runtime.last_skip_reason = "ocr-min-interval"
        return "", "", 0.0, 0.0, True, False

    frame_started = time.time()
    frame = config.decoded_frame_cache.get(frame_key)
    if frame is None:
        cap.set(cv2_module.CAP_PROP_POS_MSEC, max(0.0, sec) * 1000)
        ok, frame = cap.read()
        if not ok or frame is None:
            config.runtime.ocr_skipped += 1
            config.runtime.last_skip_reason = "frame-read-failed"
            return "", "", 0.0, time.time() - frame_started, False, False
        config.decoded_frame_cache[frame_key] = frame
        if len(config.decoded_frame_cache) > config.decoded_frame_cache_size:
            config.decoded_frame_cache.pop(next(iter(config.decoded_frame_cache)))

    if apply_brightness_gate and config.brightness_gate and config.brightness_gate_mode != "none":
        gate_passed, gate_reason = brightness_gate_result(cv2_module, frame, config)
        config.runtime.last_gate_reason = gate_reason
        if gate_reason not in config.runtime.gate_reasons:
            config.runtime.gate_reasons.append(gate_reason)
        if not gate_passed:
            config.runtime.ocr_skipped += 1
            config.runtime.gate_skipped += 1
            config.runtime.last_skip_reason = gate_reason
            return "", "", 0.0, time.time() - frame_started, True, True

    crop = _crop_frame(cv2_module, frame, roi, config.ocr_width)
    frame_elapsed = time.time() - frame_started
    config.runtime.ocr_requests += 1
    config.runtime.last_request_sec = sec
    text, scores, elapsed = _predict_text(ocr, crop)
    config.runtime.ocr_successes += 1
    config.frame_cache[cache_key] = (text, scores)
    return text, scores, elapsed, frame_elapsed, True, False


def _last_ocr_result_metadata(config: OcrConfig) -> tuple[bool, str, str]:
    return config.runtime.last_cache_hit, config.runtime.last_gate_reason, config.runtime.last_skip_reason


def _ocr_budget_method(config: OcrConfig) -> str:
    return config.runtime.last_skip_reason or "ocr-budget-exhausted"


def _result_from_crop(
    text: str,
    scores: str,
    elapsed: float,
    frame_elapsed: float,
    available: bool,
    gate_skipped: bool,
    config: OcrConfig,
) -> OcrResult:
    cache_hit, gate_reason, skip_reason = _last_ocr_result_metadata(config)
    if skip_reason == "ocr-budget-exhausted" or skip_reason == "ocr-min-interval":
        return OcrResult("", "", 0.0, _ocr_budget_method(config), frame_elapsed, cache_hit, gate_reason, skip_reason)
    if not available:
        return OcrResult("", "", 0.0, "frame-read-failed", frame_elapsed, cache_hit, gate_reason, skip_reason)
    if gate_skipped:
        return OcrResult("", "", 0.0, "opencv-no-bright-event-text", frame_elapsed, cache_hit, gate_reason, skip_reason)
    return OcrResult(text, scores, elapsed, "", frame_elapsed, cache_hit, gate_reason, skip_reason)


def _classify_ocr_result(result: OcrResult, config: OcrConfig) -> OcrResult:
    if not result.text:
        if not result.method:
            result.method = "paddle-not-target-text"
        return result
    if config.target in {"own-kill", "both"} and has_own_kill_candidate(result.text, config.language):
        if is_delayed_own_elim_text(result.text, config.language) and not extract_own_kill_events(result.text, profile=config.language):
            result.method = "paddle-own-kill-delayed-elim-skipped"
            return result
        if is_assist_own_kill_text(result.text, config.language) and not extract_own_kill_events(result.text, profile=config.language):
            result.method = "paddle-own-kill-assist-skipped"
            return result
    result.method = classify_target_text(result.text, config.target, config.language) or "paddle-not-target-text"
    return result


def ocr_at(
    cv2_module: Any,
    cap: Any,
    ocr: Any,
    sec: float,
    config: OcrConfig,
    apply_brightness_gate: bool = False,
) -> OcrResult:
    text, scores, elapsed, frame_elapsed, available, gate_skipped = _ocr_crop_at(
        cv2_module, cap, ocr, sec, config, config.roi, "primary", apply_brightness_gate
    )
    result = _result_from_crop(text, scores, elapsed, frame_elapsed, available, gate_skipped, config)
    return _classify_ocr_result(result, config)


def ocr_assist_at(cv2_module: Any, cap: Any, ocr: Any, sec: float, config: OcrConfig) -> OcrResult:
    text, scores, elapsed, frame_elapsed, available, gate_skipped = _ocr_crop_at(
        cv2_module, cap, ocr, sec, config, config.assist_roi, "assist"
    )
    result = _result_from_crop(text, scores, elapsed, frame_elapsed, available, gate_skipped, config)
    if result.method in {"", "paddle-not-target-text"}:
        result.method = "paddle-own-kill-assist-skipped" if has_assist_text(text, config.language) else "paddle-not-assist-text"
    return result


def detect_assist_nearby(
    cv2_module: Any,
    cap: Any,
    ocr: Any,
    event_sec: float,
    duration: float,
    config: OcrConfig,
    event: TextEvent,
) -> tuple[OcrResult | None, int, float, float]:
    sampled = 0
    total_ocr_seconds = 0.0
    total_frame_seconds = 0.0
    end = min(duration, event_sec + config.assist_after)
    assist_started = time.time()
    assist_used = 0
    for sec in time_range(event_sec, end, config.assist_step):
        if config.assist_max_frames is not None and assist_used >= config.assist_max_frames:
            config.runtime.termination_reason = "assist-budget-exhausted"
            break
        if config.assist_max_seconds is not None and time.time() - assist_started >= config.assist_max_seconds:
            config.runtime.termination_reason = "assist-time-budget-exhausted"
            break
        assist_used += 1
        config.runtime.assist_budget_used += 1
        config.runtime.assist_calls += 1
        sampled += 1
        result = ocr_assist_at(cv2_module, cap, ocr, sec, config)
        config.runtime.assist_ocr_seconds += result.seconds
        total_ocr_seconds += result.seconds
        total_frame_seconds += result.frame_seconds
        if is_assist_own_kill_text(result.text, config.language) and any(
            same_text_event(candidate, event)
            for candidate in extract_own_kill_events(result.text, allow_assist=True, profile=config.language)
        ):
            return result, sampled, total_ocr_seconds, total_frame_seconds
    return None, sampled, total_ocr_seconds, total_frame_seconds


def _refine_with_matcher(
    cv2_module: Any,
    cap: Any,
    ocr: Any,
    coarse_sec: float,
    duration: float,
    config: OcrConfig,
    matches: Callable[[OcrResult], bool],
    coarse_result: OcrResult | None = None,
) -> tuple[float, OcrResult, int, float, float]:
    lo = max(0.0, coarse_sec - config.refine_before)
    sampled = 0
    total_ocr_seconds = 0.0
    total_frame_seconds = 0.0
    refine_used = 0

    hit: tuple[float, OcrResult] | None = None
    if coarse_result is not None and matches(coarse_result):
        hit = (coarse_sec, coarse_result)
    else:
        if config.refine_max_frames is not None and refine_used >= config.refine_max_frames:
            config.runtime.termination_reason = "refine-budget-exhausted"
            return coarse_sec, OcrResult("", "", 0.0, "paddle-refine-budget-exhausted"), sampled, total_ocr_seconds, total_frame_seconds
        refine_used += 1
        config.runtime.refine_budget_used += 1
        config.runtime.refine_calls += 1
        result = ocr_at(cv2_module, cap, ocr, coarse_sec, config)
        config.runtime.refine_ocr_seconds += result.seconds
        sampled += 1
        total_ocr_seconds += result.seconds
        total_frame_seconds += result.frame_seconds
        if matches(result):
            hit = (coarse_sec, result)

    if hit is None:
        return coarse_sec, OcrResult("", "", 0.0, "paddle-refine-missed"), sampled, total_ocr_seconds, total_frame_seconds

    hit_sec, hit_result = hit
    miss_sec: float | None = None
    probe_step = max(config.refine_search_step, config.refine_step)
    refine_started = time.time()
    t = round(hit_sec - probe_step, 3)
    while t >= lo - 1e-6:
        if config.refine_max_frames is not None and refine_used >= config.refine_max_frames:
            config.runtime.termination_reason = "refine-budget-exhausted"
            break
        if config.refine_max_seconds is not None and time.time() - refine_started >= config.refine_max_seconds:
            config.runtime.termination_reason = "refine-time-budget-exhausted"
            break
        refine_used += 1
        config.runtime.refine_budget_used += 1
        config.runtime.refine_calls += 1
        sampled += 1
        result = ocr_at(cv2_module, cap, ocr, t, config)
        config.runtime.refine_ocr_seconds += result.seconds
        total_ocr_seconds += result.seconds
        total_frame_seconds += result.frame_seconds
        if matches(result):
            hit_sec, hit_result = t, result
            t = round(t - probe_step, 3)
            continue
        miss_sec = t
        break

    if miss_sec is None:
        return hit_sec, hit_result, sampled, total_ocr_seconds, total_frame_seconds

    tolerance = max(config.refine_step, 0.001)
    while hit_sec - miss_sec > tolerance + 1e-6:
        mid = round((hit_sec + miss_sec) / 2, 3)
        if mid <= miss_sec + 1e-6 or mid >= hit_sec - 1e-6:
            break
        if config.refine_max_frames is not None and refine_used >= config.refine_max_frames:
            config.runtime.termination_reason = "refine-budget-exhausted"
            break
        if config.refine_max_seconds is not None and time.time() - refine_started >= config.refine_max_seconds:
            config.runtime.termination_reason = "refine-time-budget-exhausted"
            break
        refine_used += 1
        config.runtime.refine_budget_used += 1
        config.runtime.refine_calls += 1
        sampled += 1
        result = ocr_at(cv2_module, cap, ocr, mid, config)
        config.runtime.refine_ocr_seconds += result.seconds
        total_ocr_seconds += result.seconds
        total_frame_seconds += result.frame_seconds
        if matches(result):
            hit_sec, hit_result = mid, result
        else:
            miss_sec = mid

    return hit_sec, hit_result, sampled, total_ocr_seconds, total_frame_seconds


def refine_event(
    cv2_module: Any,
    cap: Any,
    ocr: Any,
    coarse_sec: float,
    duration: float,
    config: OcrConfig,
    coarse_result: OcrResult | None = None,
) -> tuple[float, OcrResult, int, float, float]:
    return _refine_with_matcher(
        cv2_module,
        cap,
        ocr,
        coarse_sec,
        duration,
        config,
        lambda result: bool(classify_target_text(result.text, config.target, config.language)),
        coarse_result,
    )


def refine_text_event(
    cv2_module: Any,
    cap: Any,
    ocr: Any,
    coarse_sec: float,
    duration: float,
    config: OcrConfig,
    event: TextEvent,
    coarse_result: OcrResult | None = None,
) -> tuple[float, OcrResult, int, float, float]:
    return _refine_with_matcher(
        cv2_module,
        cap,
        ocr,
        coarse_sec,
        duration,
        config,
        lambda result: any(
            same_text_event(candidate, event)
            for candidate in extract_text_events(result.text, event.target, config.language)
        ),
        coarse_result,
    )


def detect_event(path: Path, cv2_module: Any, ocr: Any, duration: float, candidate_times: list[float], config: OcrConfig) -> EventDetection:
    return detect_events(path, cv2_module, ocr, duration, candidate_times, config)[0]


def detect_events(path: Path, cv2_module: Any, ocr: Any, duration: float, candidate_times: list[float], config: OcrConfig) -> list[EventDetection]:
    detect_started = time.time()
    config.reset_runtime()
    config.frame_cache.clear()
    config.decoded_frame_cache.clear()
    cap = cv2_module.VideoCapture(str(path))
    sampled_count = 0
    coarse_count = 0
    refine_count = 0
    gate_skipped_count = 0
    total_ocr_seconds = 0.0
    total_frame_seconds = 0.0
    last_text = ""
    last_scores = ""
    last_method = "not-scanned"
    seen_events: list[tuple[TextEvent, float]] = []
    detections: list[EventDetection] = []
    try:
        scan_times = (
            build_adaptive_scan_times(duration, candidate_times, config)
            if config.sampling_mode == "adaptive"
            else build_text_priority_scan_times(duration, candidate_times, config)
        )
        scheduled = {int(round(sec * 1000)) for sec in scan_times}
        adaptive_seed_keys = set(scheduled) if config.sampling_mode == "adaptive" else set()
        scan_index = 0
        while scan_index < len(scan_times):
            sec = scan_times[scan_index]
            scan_index += 1
            sampled_count += 1
            frame_key = int(round(sec * 1000))
            is_adaptive_seed = frame_key in adaptive_seed_keys
            if is_adaptive_seed or config.sampling_mode != "adaptive":
                coarse_count += 1
            result = (
                ocr_at(cv2_module, cap, ocr, sec, config, apply_brightness_gate=True)
                if config.brightness_gate
                else ocr_at(cv2_module, cap, ocr, sec, config)
            )
            total_ocr_seconds += result.seconds
            total_frame_seconds += result.frame_seconds
            if result.method == "opencv-no-bright-event-text":
                gate_skipped_count += 1
            if result.skipped_reason == "ocr-budget-exhausted":
                break
            if result.text:
                last_text, last_scores, last_method = result.text, result.scores, result.method
            if (
                is_adaptive_seed
                and extract_text_events(result.text, config.target, config.language)
            ):
                for dense_sec in time_range(
                    max(config.scan_start, sec),
                    min(duration, sec + config.adaptive_window),
                    config.adaptive_step,
                ):
                    dense_key = int(round(dense_sec * 1000))
                    if dense_key not in scheduled:
                        insort(scan_times, dense_sec)
                        scheduled.add(dense_key)
            for event in extract_text_events(result.text, config.target, config.language):
                if any(
                    same_text_event(event, seen)
                    or (
                        abs(sec - seen_sec) <= config.event_dedupe_seconds
                        and same_noisy_close_event(event, seen)
                    )
                    for seen, seen_sec in seen_events
                ):
                    continue
                target_config = replace(config, target=event.target)
                event_sec, refined, refined_count, refined_seconds, refined_frame_seconds = refine_text_event(
                    cv2_module, cap, ocr, sec, duration, target_config, event, result
                )
                sampled_count += refined_count
                refine_count += refined_count
                total_ocr_seconds += refined_seconds
                total_frame_seconds += refined_frame_seconds
                refined_events = [
                    candidate
                    for candidate in extract_text_events(refined.text, event.target, config.language)
                    if same_text_event(candidate, event)
                ]
                refined_event = refined_events[0] if refined_events else event
                if any(
                    abs(event_sec - seen_sec) <= config.event_dedupe_seconds
                    and same_noisy_close_event(refined_event, seen)
                    for seen, seen_sec in seen_events
                ):
                    continue
                followup_skip_method = _followup_elimination_skip_method(
                    refined_event,
                    seen_events,
                    config.language,
                    event_sec,
                )
                if followup_skip_method:
                    last_text = refined.text or result.text
                    last_scores = refined.scores or result.scores
                    last_method = followup_skip_method
                    seen_events.append((refined_event, event_sec))
                    continue
                method = refined_event.method
                if event.target == "own-kill":
                    assist_result, assist_count, assist_seconds, assist_frame_seconds = detect_assist_nearby(
                        cv2_module, cap, ocr, event_sec, duration, target_config, refined_event
                    )
                    sampled_count += assist_count
                    total_ocr_seconds += assist_seconds
                    total_frame_seconds += assist_frame_seconds
                    if assist_result is not None:
                        last_text = normalize_text(" ".join(part for part in [refined.text or result.text, assist_result.text] if part))
                        last_scores = ";".join(part for part in [refined.scores or result.scores, assist_result.scores] if part)
                        last_method = "paddle-own-kill-assist-skipped"
                        seen_events.append((event, event_sec))
                        continue
                detections.append(
                    EventDetection(
                        duration,
                        event_sec,
                        method,
                        "ocr",
                        refined.text or result.text,
                        refined.scores or result.scores,
                        total_ocr_seconds,
                        sampled_count,
                        time.time() - detect_started,
                        total_frame_seconds,
                        coarse_count,
                        refine_count,
                        event.target,
                        refined_event.key,
                        "1",
                        f"{event_sec:.3f}",
                        event_weapon=refined_event.weapon,
                        ocr_gate_skipped_frames=gate_skipped_count,
                        ocr_requests=config.runtime.ocr_requests,
                        ocr_successes=config.runtime.ocr_successes,
                        ocr_cache_hits=config.runtime.ocr_cache_hits,
                        ocr_skipped=config.runtime.ocr_skipped,
                        refine_budget_used=config.runtime.refine_budget_used,
                        assist_budget_used=config.runtime.assist_budget_used,
                        termination_reason=config.runtime.termination_reason,
                        gate_reason=";".join(config.runtime.gate_reasons) or ("disabled" if not config.brightness_gate else ""),
                        refine_ocr_seconds=config.runtime.refine_ocr_seconds,
                        assist_ocr_seconds=config.runtime.assist_ocr_seconds,
                    )
                )
                seen_events.append((refined_event, event_sec))
    finally:
        cap.release()

    if detections:
        return detections
    return [
        EventDetection(
            duration,
            None,
            last_method if last_method != "not-scanned" else "paddle-no-target-text-found",
            "ocr",
            last_text,
            last_scores,
            total_ocr_seconds,
            sampled_count,
            time.time() - detect_started,
            total_frame_seconds,
            coarse_count,
            refine_count,
            config.target,
            ocr_gate_skipped_frames=gate_skipped_count,
            ocr_requests=config.runtime.ocr_requests,
            ocr_successes=config.runtime.ocr_successes,
            ocr_cache_hits=config.runtime.ocr_cache_hits,
            ocr_skipped=config.runtime.ocr_skipped,
            refine_budget_used=config.runtime.refine_budget_used,
            assist_budget_used=config.runtime.assist_budget_used,
            termination_reason=config.runtime.termination_reason,
            gate_reason=";".join(config.runtime.gate_reasons) or ("disabled" if not config.brightness_gate else ""),
            refine_ocr_seconds=config.runtime.refine_ocr_seconds,
            assist_ocr_seconds=config.runtime.assist_ocr_seconds,
        )
    ]
