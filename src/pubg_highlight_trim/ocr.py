from __future__ import annotations

import os
import re
import time
from dataclasses import dataclass, field, replace
from difflib import SequenceMatcher
from pathlib import Path
from typing import Any, Callable

from .models import EventDetection
from .runtime import first_existing_runtime_path

os.environ.setdefault("PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK", "True")

SELF_STRICT_RE = re.compile(r"(击倒了你|淘汰了你)")
SELF_ZONE_DOWNED_RE = re.compile(r"(你在安全区外倒地了|安全区外倒地了|安全区外倒地)")
SELF_FUZZY_RE = re.compile(r"(击倒.{0,2}你|淘.{0,2}了?你|倒了你)")
OWN_KILL_STRICT_RE = re.compile(r"(?:^|[，。:：])?你.{0,24}(?:击倒|淘汰)了(?!你)")
OWN_KILL_EVENT_RE = re.compile(r"你.{0,24}?(?P<action>击倒|淘汰)了(?P<victim>.+?)(?=你.{0,24}?(?:击倒|淘汰)了|$)")
OWN_KILL_ASSIST_RE = re.compile(r"(协助次数|协助|助.{0,2}攻)")
OWN_KILL_DELAYED_ELIM_RE = re.compile(r"你终于淘汰了")
OWN_KILL_VICTIM_STOP_RE = re.compile(r"(淘汰数?\d*协助次数|淘汰\d+协助次数|淘汰数|协助次数|协助|助.{0,2}攻|你用|击倒了你|淘汰了你)")
WEAPON_PREFIX_RE = re.compile(r"(?:你)?(?:使用|用)(?P<weapon>.+)$")
SELF_WEAPON_RE = re.compile(r"(?:使用|用)(?P<weapon>[^用]{1,32})$")
MOLOTOV_WEAPON_RE = re.compile(r"(燃烧弹|燃燒彈|燃烧瓶|燃燒瓶|molotov)", re.IGNORECASE)


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


def _clean_subject(text: str) -> str:
    text = normalize_text(text)
    text = OWN_KILL_VICTIM_STOP_RE.split(text, maxsplit=1)[0]
    text = text.strip("，。,.、:：;；|/\\()（）[]【】 ")
    return text[:48]


def _subject_key(text: str) -> str:
    cleaned = _clean_subject(text)
    key = "".join(ch.lower() for ch in cleaned if ch.isalnum() or ch in "_-[]")
    return key or normalize_text(cleaned).lower()


def _clean_weapon(text: str) -> str:
    text = normalize_text(text)
    text = text.strip("，。,.、:：;；|/\\()（）[]【】 ")
    return text[:32]


def _own_kill_weapon(raw: str, action: str) -> str:
    prefix = normalize_text(raw).split(f"{action}了", 1)[0]
    match = WEAPON_PREFIX_RE.search(prefix)
    return _clean_weapon(match.group("weapon")) if match else ""


def _self_event_weapon(actor_text: str) -> str:
    match = SELF_WEAPON_RE.search(normalize_text(actor_text))
    return _clean_weapon(match.group("weapon")) if match else ""


def is_molotov_weapon(weapon: str) -> bool:
    return bool(MOLOTOV_WEAPON_RE.search(normalize_text(weapon)))


def extract_own_kill_events(text: str, allow_assist: bool = False) -> list[TextEvent]:
    text = normalize_text(text)
    if not text:
        return []
    events: list[TextEvent] = []
    for match in OWN_KILL_EVENT_RE.finditer(text):
        raw = normalize_text(match.group(0))
        if is_delayed_own_elim_text(raw):
            continue
        if not allow_assist and is_assist_own_kill_text(raw):
            continue
        subject = _clean_subject(match.group("victim"))
        subject_key = _subject_key(subject)
        if not subject_key or subject_key in {"你", "ni"}:
            continue
        action = match.group("action")
        weapon = _own_kill_weapon(raw, action)
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


def extract_self_events(text: str) -> list[TextEvent]:
    text = normalize_text(text)
    method = classify_self_text(text)
    if not method:
        return []
    if SELF_ZONE_DOWNED_RE.search(text):
        return [TextEvent("self-death", method, "zone-downed", "zone", "self-death:zone", text)]
    action_match = SELF_STRICT_RE.search(text) or SELF_FUZZY_RE.search(text)
    action = action_match.group(1) if action_match else "self-death"
    actor = text[: action_match.start()] if action_match else text
    actor_key = _subject_key(actor[-32:]) or "unknown"
    weapon = _self_event_weapon(actor)
    return [TextEvent("self-death", method, action, actor, f"self-death:{action}:{actor_key}", text, weapon)]


def extract_text_events(text: str, target: str) -> list[TextEvent]:
    events: list[TextEvent] = []
    if target in {"self-death", "both"}:
        events.extend(extract_self_events(text))
    if target in {"own-kill", "both"}:
        events.extend(extract_own_kill_events(text))
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


def same_noisy_close_event(left: TextEvent, right: TextEvent) -> bool:
    if same_text_event(left, right):
        return True
    if left.target != right.target or left.action != right.action:
        return False
    _, _, left_subject = _split_event_key(left)
    _, _, right_subject = _split_event_key(right)
    if not left_subject or not right_subject:
        return False
    if left.target == "self-death":
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


def _self_death_actor_anchor(subject: str) -> str:
    ascii_key = _latin_subject_key(subject)
    match = re.search(r"\[[a-z0-9]+\][a-z0-9_-]{3,}", ascii_key)
    return match.group(0) if match else ""


def _latin_subject_key(subject: str) -> str:
    return "".join(ch.lower() for ch in subject if ch.isascii() and (ch.isalnum() or ch in "_-[]"))


def _compact_latin_subject_key(subject: str) -> str:
    return "".join(ch.lower() for ch in subject if ch.isascii() and ch.isalnum())


def classify_own_kill_text(text: str) -> str | None:
    events = extract_own_kill_events(text)
    return events[0].method if events else None


def has_assist_text(text: str) -> bool:
    return bool(OWN_KILL_ASSIST_RE.search(normalize_text(text)))


def is_delayed_own_elim_text(text: str) -> bool:
    return bool(OWN_KILL_DELAYED_ELIM_RE.search(normalize_text(text)))


def is_assist_own_kill_text(text: str) -> bool:
    text = normalize_text(text)
    assist = OWN_KILL_ASSIST_RE.search(text)
    if not assist:
        return False
    if re.search(r"淘汰数?\d*协助次数|淘汰\d+协助次数", text):
        return False
    kill_count = re.search(r"淘汰数", text)
    return not kill_count or assist.start() < kill_count.start()


def has_own_kill_candidate(text: str) -> bool:
    return bool(OWN_KILL_STRICT_RE.search(normalize_text(text)))


def classify_target_kind(text: str, target: str) -> tuple[str, str] | None:
    events = extract_text_events(text, target)
    return (events[0].method, events[0].target) if events else None


def classify_target_text(text: str, target: str) -> str | None:
    classified = classify_target_kind(text, target)
    return classified[0] if classified else None


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


def _crop_frame(cv2_module: Any, frame: Any, roi: tuple[float, float, float, float], ocr_width: int) -> Any:
    h, w = frame.shape[:2]
    x1, y1, x2, y2 = roi
    crop = frame[int(h * y1) : int(h * y2), int(w * x1) : int(w * x2)]
    if ocr_width > 0 and crop.shape[1] > ocr_width:
        scale = ocr_width / crop.shape[1]
        crop = cv2_module.resize(crop, (ocr_width, max(1, int(crop.shape[0] * scale))), interpolation=cv2_module.INTER_AREA)
    return crop


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


def ocr_at(cv2_module: Any, cap: Any, ocr: Any, sec: float, config: OcrConfig) -> OcrResult:
    frame_started = time.time()
    cap.set(cv2_module.CAP_PROP_POS_MSEC, max(0.0, sec) * 1000)
    ok, frame = cap.read()
    if not ok or frame is None:
        return OcrResult("", "", 0.0, "frame-read-failed", time.time() - frame_started)

    crop = _crop_frame(cv2_module, frame, config.roi, config.ocr_width)
    frame_elapsed = time.time() - frame_started

    text, scores, elapsed = _predict_text(ocr, crop)
    if config.target in {"own-kill", "both"} and has_own_kill_candidate(text):
        if is_delayed_own_elim_text(text) and not extract_own_kill_events(text):
            method = "paddle-own-kill-delayed-elim-skipped"
            return OcrResult(text, scores, elapsed, method, frame_elapsed)
        if is_assist_own_kill_text(text) and not extract_own_kill_events(text):
            method = "paddle-own-kill-assist-skipped"
            return OcrResult(text, scores, elapsed, method, frame_elapsed)

    method = classify_target_text(text, config.target) or "paddle-not-target-text"
    return OcrResult(text, scores, elapsed, method, frame_elapsed)


def ocr_assist_at(cv2_module: Any, cap: Any, ocr: Any, sec: float, config: OcrConfig) -> OcrResult:
    frame_started = time.time()
    cap.set(cv2_module.CAP_PROP_POS_MSEC, max(0.0, sec) * 1000)
    ok, frame = cap.read()
    if not ok or frame is None:
        return OcrResult("", "", 0.0, "frame-read-failed", time.time() - frame_started)

    crop = _crop_frame(cv2_module, frame, config.assist_roi, config.ocr_width)
    frame_elapsed = time.time() - frame_started
    text, scores, elapsed = _predict_text(ocr, crop)
    method = "paddle-own-kill-assist-skipped" if has_assist_text(text) else "paddle-not-assist-text"
    return OcrResult(text, scores, elapsed, method, frame_elapsed)


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
    for sec in time_range(event_sec, end, config.assist_step):
        sampled += 1
        result = ocr_assist_at(cv2_module, cap, ocr, sec, config)
        total_ocr_seconds += result.seconds
        total_frame_seconds += result.frame_seconds
        if is_assist_own_kill_text(result.text) and any(
            same_text_event(candidate, event) for candidate in extract_own_kill_events(result.text, allow_assist=True)
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

    hit: tuple[float, OcrResult] | None = None
    if coarse_result is not None and matches(coarse_result):
        hit = (coarse_sec, coarse_result)
    else:
        result = ocr_at(cv2_module, cap, ocr, coarse_sec, config)
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
    t = round(hit_sec - probe_step, 3)
    while t >= lo - 1e-6:
        sampled += 1
        result = ocr_at(cv2_module, cap, ocr, t, config)
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
        sampled += 1
        result = ocr_at(cv2_module, cap, ocr, mid, config)
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
        lambda result: bool(classify_target_text(result.text, config.target)),
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
        lambda result: any(same_text_event(candidate, event) for candidate in extract_text_events(result.text, event.target)),
        coarse_result,
    )


def detect_event(path: Path, cv2_module: Any, ocr: Any, duration: float, candidate_times: list[float], config: OcrConfig) -> EventDetection:
    return detect_events(path, cv2_module, ocr, duration, candidate_times, config)[0]


def detect_events(path: Path, cv2_module: Any, ocr: Any, duration: float, candidate_times: list[float], config: OcrConfig) -> list[EventDetection]:
    detect_started = time.time()
    cap = cv2_module.VideoCapture(str(path))
    sampled_count = 0
    coarse_count = 0
    refine_count = 0
    total_ocr_seconds = 0.0
    total_frame_seconds = 0.0
    last_text = ""
    last_scores = ""
    last_method = "not-scanned"
    seen_events: list[tuple[TextEvent, float]] = []
    detections: list[EventDetection] = []
    try:
        for sec in build_text_priority_scan_times(duration, candidate_times, config):
            sampled_count += 1
            coarse_count += 1
            result = ocr_at(cv2_module, cap, ocr, sec, config)
            total_ocr_seconds += result.seconds
            total_frame_seconds += result.frame_seconds
            if result.text:
                last_text, last_scores, last_method = result.text, result.scores, result.method
            for event in extract_text_events(result.text, config.target):
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
                    candidate for candidate in extract_text_events(refined.text, event.target) if same_text_event(candidate, event)
                ]
                refined_event = refined_events[0] if refined_events else event
                if any(
                    abs(event_sec - seen_sec) <= config.event_dedupe_seconds
                    and same_noisy_close_event(refined_event, seen)
                    for seen, seen_sec in seen_events
                ):
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
        )
    ]
