from __future__ import annotations

import subprocess
from pathlib import Path

from .ffmpeg_tools import duration_sec
from .models import EventDetection

W, H, FPS = 384, 240, 10
HEALTH_X0, HEALTH_X1 = 145, 245
HEALTH_Y0, HEALTH_Y1 = 225, 238


def health_state(buf: bytes) -> tuple[str, float]:
    red = red_soft = white = yellow = blue = bright = dark = total = 0
    for y in range(HEALTH_Y0, HEALTH_Y1):
        base = y * W * 3
        for x in range(HEALTH_X0, HEALTH_X1):
            off = base + x * 3
            r, g, b = buf[off], buf[off + 1], buf[off + 2]
            mx, mn = max(r, g, b), min(r, g, b)
            if r > 145 and g < 95 and b < 95:
                red += 1
            if r > 95 and r > g + 18 and r > b + 18 and g < 140 and b < 140:
                red_soft += 1
            if r > 185 and g > 185 and b > 185 and mx - mn < 45:
                white += 1
            if r > 165 and g > 125 and b < 145 and r >= g:
                yellow += 1
            if b > 130 and g > 80 and r < 130:
                blue += 1
            if mx > 160 and mx - mn < 95:
                bright += 1
            if mx < 80:
                dark += 1
            total += 1
    red_ratio = red / total
    red_soft_ratio = red_soft / total
    white_ratio = white / total
    yellow_ratio = yellow / total
    blue_ratio = blue / total
    bright_ratio = bright / total
    dark_ratio = dark / total
    if red_ratio > 0.075 and yellow_ratio < 0.02 and bright_ratio < 0.05:
        return "red", max(red_ratio, red_soft_ratio)
    if red_soft_ratio > 0.055 and yellow_ratio < 0.02 and bright_ratio < 0.05 and dark_ratio > 0.85:
        return "muted-red-downed", red_soft_ratio
    if white_ratio > 0.018 or yellow_ratio > 0.018 or blue_ratio > 0.018 or bright_ratio > 0.045:
        return "present", max(red_ratio, red_soft_ratio)
    return "absent", max(red_ratio, red_soft_ratio)


def already_downed_window(states: list[tuple[float, str, float]], start: float, end: float) -> bool:
    window = [(state, score) for t, state, score in states if start <= t < end]
    if len(window) < 8:
        return False
    downed_states = {"red", "muted-red-downed"}
    if sum(1 for state, _ in window if state in downed_states) / len(window) > 0.65:
        return True

    scores = [score for _, score in window]
    third = max(1, len(scores) // 3)
    first = sum(scores[:third]) / third
    last = sum(scores[-third:]) / third
    high_red = sum(1 for score in scores if score > 0.18) / len(scores)
    return high_red > 0.45 and first > last + 0.05


def sample_health_states(path: Path, ffmpeg: str, fps: float, until: float | None = None) -> list[tuple[float, str, float]]:
    cmd = [
        ffmpeg,
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        str(path),
    ]
    if until is not None:
        cmd.extend(["-t", f"{until:.3f}"])
    cmd.extend(["-vf", f"fps={fps},scale={W}:{H}", "-pix_fmt", "rgb24", "-f", "rawvideo", "-"])
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    frame_size = W * H * 3
    idx = 0
    states: list[tuple[float, str, float]] = []
    try:
        while True:
            buf = proc.stdout.read(frame_size) if proc.stdout else b""
            if len(buf) < frame_size:
                break
            t = idx / fps
            idx += 1
            state, red_score = health_state(buf)
            states.append((t, state, red_score))
    finally:
        try:
            proc.kill()
        except Exception:
            pass
    return states


def opening_already_downed(
    path: Path,
    ffmpeg: str,
    check_start: float = 0.5,
    check_end: float = 3.0,
    fps: float = 5.0,
    red_threshold: float = 0.65,
) -> tuple[bool, float, int]:
    states = sample_health_states(path, ffmpeg, fps=fps, until=check_end)
    window = [(state, score) for t, state, score in states if check_start <= t < check_end]
    if not window:
        return False, 0.0, 0
    red_ratio = sum(1 for state, _ in window if state in {"red", "muted-red-downed"}) / len(window)
    return red_ratio >= red_threshold, red_ratio, len(window)


def detect_event(path: Path, ffmpeg: str, ffprobe: str) -> EventDetection:
    dur = duration_sec(path, ffprobe)
    states = [(t, state, score) for t, state, score in sample_health_states(path, ffmpeg, fps=FPS) if t >= 2.0]

    if already_downed_window(states, 2.0, 4.0):
        return EventDetection(dur, None, "skipped-starts-already-downed", "health", sampled_count=len(states))

    red_times = [t for t, state, _ in states if state == "red"]
    for t in red_times:
        if sum(1 for u in red_times if t <= u < t + 1.1) >= 9:
            trim_start = max(0.0, t - 5.0)
            if already_downed_window(states, trim_start, trim_start + 2.0):
                return EventDetection(dur, None, "skipped-trim-starts-already-downed", "health", sampled_count=len(states))
            return EventDetection(dur, t, "own-knock-or-elim-red-healthbar", "health", sampled_count=len(states))

    seen_health = False
    for i, (t, state, _) in enumerate(states):
        if state in {"present", "red", "muted-red-downed"}:
            seen_health = True
            continue
        if not seen_health:
            continue
        window = [s for u, s, _ in states[i:] if t <= u < t + 1.5]
        later = [s for _, s, _ in states[i:]]
        if len(window) >= 12 and all(s == "absent" for s in window) and not any(s in {"present", "red", "muted-red-downed"} for s in later):
            trim_start = max(0.0, t - 5.0)
            if already_downed_window(states, trim_start, trim_start + 2.0):
                return EventDetection(dur, None, "skipped-trim-starts-already-downed", "health", sampled_count=len(states))
            return EventDetection(dur, t, "direct-elim-healthbar-disappeared", "health", sampled_count=len(states))

    return EventDetection(dur, None, "skipped-healthbar-evidence-not-found", "health", sampled_count=len(states))
