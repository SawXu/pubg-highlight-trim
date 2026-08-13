from __future__ import annotations

import csv
import ctypes
import json
import os
import shutil
import sys
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace

from .ffmpeg_tools import concat_clips, duration_sec, find_ffmpeg_pair, trim_clip
from .game_languages import (
    AUTO_GAME_LANGUAGE,
    DEFAULT_GAME_LANGUAGE,
    GameLanguageProfile,
    default_game_language_profile,
    game_language_choices,
    get_game_language_profile,
)
from .models import EventDetection
from .ocr import OcrConfig, OcrUnavailable, detect_events as detect_ocr_events, is_molotov_weapon, load_backend
from .runtime import suppress_process_output
from .source_files import infer_source_file_languages, iter_source_file_languages, iter_source_files


_WORKER_OCR_BACKENDS: dict[str, tuple[object, object]] = {}


def unique_path(path: Path) -> Path:
    if not path.exists():
        return path
    for i in range(1, 1000):
        candidate = path.with_name(f"{path.stem}_{i}{path.suffix}")
        if not candidate.exists():
            return candidate
    raise RuntimeError(f"Could not create unique path for {path}")


def unique_dir(path: Path) -> Path:
    if not path.exists():
        return path
    for i in range(1, 1000):
        candidate = path.with_name(f"{path.name}_{i}")
        if not candidate.exists():
            return candidate
    raise RuntimeError(f"Could not create unique directory for {path}")


def read_candidate_csv(path: Path | None) -> dict[str, list[float]]:
    if not path or not path.exists():
        return {}
    out: dict[str, list[float]] = {}
    with path.open("r", encoding="utf-8-sig", newline="") as handle:
        for row in csv.DictReader(handle):
            name = row.get("Name", "")
            value = row.get("OcrEventSec") or row.get("EventSec") or ""
            if not name or not value:
                continue
            try:
                out.setdefault(name, []).append(float(value))
            except ValueError:
                pass
    return out


def _discover_candidate_csv(input_path: Path, base_folder: Path) -> Path | None:
    roots = [base_folder]
    if input_path.is_dir():
        roots.append(input_path)
    roots.append(base_folder.parent)

    candidates: list[Path] = []
    for root in dict.fromkeys(roots):
        if not root.exists():
            continue
        candidates.extend(path for path in root.glob("fullscan_*/candidate_events.csv") if path.is_file())
    if not candidates:
        return None
    return max(candidates, key=lambda path: path.stat().st_mtime)


def _candidate_csv_path(args: SimpleNamespace, input_path: Path, base_folder: Path) -> Path | None:
    explicit = getattr(args, "candidate_csv", None)
    if explicit:
        return explicit
    if getattr(args, "no_auto_candidate_csv", False):
        return None
    return _discover_candidate_csv(input_path, base_folder)


def _use_fast_scan(args: SimpleNamespace, candidate_csv: Path | None) -> bool:
    mode = getattr(args, "scan_mode", "auto")
    if mode == "fast":
        return True
    if mode == "full":
        return False
    return candidate_csv is not None and candidate_csv.exists()


def _source_forces_full_scan(src: Path, language: GameLanguageProfile) -> bool:
    return (
        language.multi_kill_source_re.search(src.name) is not None
        or language.match_end_source_re.search(src.name) is not None
    )


def _ocr_config_for_source(config: OcrConfig, src: Path, language: GameLanguageProfile) -> OcrConfig:
    updates: dict[str, object] = {}
    if config.no_full_scan and _source_forces_full_scan(src, language):
        updates["no_full_scan"] = False
    if language.match_end_source_re.search(src.name) is not None:
        updates["brightness_gate"] = False
    return replace(config, **updates) if updates else config


def _effective_no_merge(args: SimpleNamespace, single_file: bool) -> bool:
    merge = getattr(args, "merge", None)
    if merge is None:
        return single_file
    return merge is False


def _merge_output_override(args: SimpleNamespace) -> Path | None:
    merge = getattr(args, "merge", None)
    if isinstance(merge, (str, Path)):
        return Path(merge)
    return None


def _requested_game_language(args: SimpleNamespace) -> str:
    return getattr(args, "game_lang", AUTO_GAME_LANGUAGE)


def _is_auto_game_language(args: SimpleNamespace) -> bool:
    return _requested_game_language(args) == AUTO_GAME_LANGUAGE


def _format_language_counts(file_profiles: dict[Path, GameLanguageProfile]) -> str:
    counts = Counter(profile.code for profile in file_profiles.values())
    return ",".join(f"{code}:{counts[code]}" for code in sorted(counts))


def _select_input_files(
    input_paths: list[Path],
    args: SimpleNamespace,
    directory_input: bool,
) -> tuple[list[Path], dict[Path, GameLanguageProfile]]:
    target = getattr(args, "target", "self-death")
    if _is_auto_game_language(args):
        if not directory_input:
            profiles = infer_source_file_languages(input_paths, target)
            fallback = default_game_language_profile()
            return input_paths, {path: profiles.get(path, fallback) for path in input_paths}
        input_path = input_paths[0]
        selections = iter_source_file_languages(input_path, recursive=args.recursive, target=target)
        return [path for path, _ in selections], dict(selections)

    profile = get_game_language_profile(_requested_game_language(args) or DEFAULT_GAME_LANGUAGE)
    if not directory_input:
        return input_paths, {path: profile for path in input_paths}
    input_path = input_paths[0]
    files = iter_source_files(input_path, recursive=args.recursive, target=target, language=profile)
    return files, {path: profile for path in files}


def _input_paths(args: SimpleNamespace) -> list[Path]:
    explicit_files = getattr(args, "files", None)
    positional_input = getattr(args, "input", None)
    if explicit_files and positional_input is not None:
        raise SystemExit("Use either the positional input or --files, not both.")
    raw_inputs = explicit_files or [positional_input or Path(".")]
    return [Path(path).resolve() for path in raw_inputs]


def _validate_inputs(args: SimpleNamespace) -> tuple[list[Path], bool, bool, Path]:
    input_paths = _input_paths(args)
    for input_path in input_paths:
        if not input_path.exists():
            raise SystemExit(f"Input path does not exist: {input_path}")

    if len(input_paths) == 1 and input_paths[0].is_dir():
        return input_paths, True, False, input_paths[0]

    for input_path in input_paths:
        if not input_path.is_file():
            raise SystemExit(f"Multiple inputs must all be mp4 files: {input_path}")
        if input_path.suffix.lower() != ".mp4":
            label = "Single-file input" if len(input_paths) == 1 else "Multiple inputs"
            raise SystemExit(f"{label} must be .mp4 files: {input_path}")

    single_file = len(input_paths) == 1
    base_folder = input_paths[0].parent
    return input_paths, False, single_file, base_folder


def _prepare_output_paths(
    args: SimpleNamespace,
    input_path: Path,
    input_files: list[Path],
    base_folder: Path,
    single_file: bool,
) -> tuple[Path, Path]:
    merge_output_override = _merge_output_override(args)
    if single_file:
        outdir = args.output_dir or base_folder / f"{input_path.stem}_pubg_trim_clips"
        merged = merge_output_override or base_folder / f"{input_path.stem}_pubg_trim.mp4"
    else:
        outdir = args.output_dir or base_folder / "pubg_highlight_trim_output"
        merged = merge_output_override or base_folder / "pubg_highlight_trim_merged.mp4"

    if merged.resolve() in {path.resolve() for path in input_files}:
        raise SystemExit(f"Merge output must not overwrite an input file: {merged}")

    if args.overwrite:
        if outdir.exists():
            shutil.rmtree(outdir)
        if merged.exists():
            merged.unlink()
    else:
        outdir = unique_dir(outdir)
        merged = unique_path(merged)

    outdir.mkdir(parents=True, exist_ok=True)
    merged.parent.mkdir(parents=True, exist_ok=True)
    return outdir, merged


def _blank_record(idx: int, src: Path, dur: float, status: str, method: str, detector: str) -> dict[str, str]:
    return {
        "Index": str(idx),
        "Name": src.name,
        "Target": "",
        "DurationSec": f"{dur:.3f}",
        "Status": status,
        "EventSec": "",
        "EventSecs": "",
        "EventCount": "0",
        "EventKeys": "",
        "EventWeapon": "",
        "ContextRule": "",
        "BeforeSec": "",
        "AfterSec": "",
        "KeepStartSec": "",
        "KeepEndSec": "",
        "KeepDurationSec": "",
        "Method": method,
        "Detector": detector,
        "PaddleText": "",
        "PaddleScores": "",
        "OcrSeconds": "0.000",
        "SampledFrames": "0",
        "DetectSeconds": "0.000",
        "OcrFrameSeconds": "0.000",
        "OcrCoarseFrames": "0",
        "OcrRefineFrames": "0",
        "OcrGateSkippedFrames": "0",
        "ProbeSeconds": "0.000",
        "TrimSeconds": "0.000",
        "TotalSeconds": "0.000",
        "Encoder": "",
        "Output": "",
    }


def _record_detection(
    idx: int,
    src: Path,
    detection: EventDetection,
    start: float,
    end: float,
    encoder: str,
    output: str,
    probe_seconds: float,
    trim_seconds: float,
    total_seconds: float,
) -> dict[str, str]:
    return {
        "Index": str(idx),
        "Name": src.name,
        "Target": detection.target,
        "DurationSec": f"{detection.duration_sec:.3f}",
        "Status": "included",
        "EventSec": f"{detection.event_sec:.3f}" if detection.event_sec is not None else "",
        "EventSecs": detection.event_secs or (f"{detection.event_sec:.3f}" if detection.event_sec is not None else ""),
        "EventCount": detection.event_count,
        "EventKeys": detection.event_key,
        "EventWeapon": detection.event_weapon,
        "ContextRule": detection.context_rule,
        "BeforeSec": f"{_detection_before(detection):.3f}",
        "AfterSec": f"{_detection_after(detection):.3f}",
        "KeepStartSec": f"{start:.3f}",
        "KeepEndSec": f"{end:.3f}",
        "KeepDurationSec": f"{end - start:.3f}",
        "Method": detection.method,
        "Detector": detection.detector,
        "PaddleText": detection.text,
        "PaddleScores": detection.scores,
        "OcrSeconds": f"{detection.ocr_seconds:.3f}",
        "SampledFrames": str(detection.sampled_count),
        "DetectSeconds": f"{detection.detect_seconds:.3f}",
        "OcrFrameSeconds": f"{detection.ocr_frame_seconds:.3f}",
        "OcrCoarseFrames": str(detection.ocr_coarse_frames),
        "OcrRefineFrames": str(detection.ocr_refine_frames),
        "OcrGateSkippedFrames": str(detection.ocr_gate_skipped_frames),
        "ProbeSeconds": f"{probe_seconds:.3f}",
        "TrimSeconds": f"{trim_seconds:.3f}",
        "TotalSeconds": f"{total_seconds:.3f}",
        "Encoder": encoder,
        "Output": output,
    }


def _record_skip(
    idx: int,
    src: Path,
    detection: EventDetection,
    probe_seconds: float = 0.0,
    trim_seconds: float = 0.0,
    total_seconds: float = 0.0,
) -> dict[str, str]:
    row = _blank_record(idx, src, detection.duration_sec, "skipped", detection.method, detection.detector)
    row.update(
        {
            "Target": detection.target,
            "EventSec": f"{detection.event_sec:.3f}" if detection.event_sec is not None else "",
            "EventSecs": detection.event_secs,
            "EventCount": detection.event_count if detection.event_sec is not None else "0",
            "EventKeys": detection.event_key,
            "EventWeapon": detection.event_weapon,
            "ContextRule": detection.context_rule,
            "BeforeSec": f"{detection.clip_before_sec:.3f}" if detection.clip_before_sec else "",
            "AfterSec": f"{detection.clip_after_sec:.3f}" if detection.clip_after_sec else "",
            "PaddleText": detection.text,
            "PaddleScores": detection.scores,
            "OcrSeconds": f"{detection.ocr_seconds:.3f}",
            "SampledFrames": str(detection.sampled_count),
            "DetectSeconds": f"{detection.detect_seconds:.3f}",
            "OcrFrameSeconds": f"{detection.ocr_frame_seconds:.3f}",
            "OcrCoarseFrames": str(detection.ocr_coarse_frames),
            "OcrRefineFrames": str(detection.ocr_refine_frames),
            "OcrGateSkippedFrames": str(detection.ocr_gate_skipped_frames),
            "ProbeSeconds": f"{probe_seconds:.3f}",
            "TrimSeconds": f"{trim_seconds:.3f}",
            "TotalSeconds": f"{total_seconds:.3f}",
        }
    )
    return row


def _min_event_sec(args: SimpleNamespace) -> float:
    value = getattr(args, "min_event_sec", 2.0)
    if value is None:
        return 0.0
    return max(0.0, float(value))


def _molotov_elim_before(args: SimpleNamespace) -> float:
    value = getattr(args, "molotov_elim_before", 10.0)
    if value is None:
        return 0.0
    return max(0.0, float(value))


def _early_event_skip(detection: EventDetection) -> EventDetection:
    return replace(detection, method="skipped-before-min-event-sec")


def _partition_too_early_detections(
    detections: list[EventDetection],
    min_event_sec: float,
) -> tuple[list[EventDetection], list[EventDetection]]:
    kept: list[EventDetection] = []
    skipped: list[EventDetection] = []
    for detection in detections:
        if detection.event_sec is not None and detection.event_sec < min_event_sec:
            skipped.append(_early_event_skip(detection))
        else:
            kept.append(detection)
    return kept, skipped


def _detection_before(detection: EventDetection) -> float:
    return max(0.0, detection.clip_before_sec)


def _detection_after(detection: EventDetection) -> float:
    return max(0.0, detection.clip_after_sec)


def _has_canonical_action(detection: EventDetection, action: str) -> bool:
    for key in detection.event_key.split(";"):
        parts = key.split(":", 2)
        if len(parts) >= 2 and parts[1] == action:
            return True
    return False


def _is_molotov_elim_detection(detection: EventDetection, profile: GameLanguageProfile | None = None) -> bool:
    return (
        detection.event_sec is not None
        and _has_canonical_action(detection, "eliminate")
        and is_molotov_weapon(detection.event_weapon, profile)
    )


def _apply_context_rules(
    detection: EventDetection,
    args: SimpleNamespace,
    profile: GameLanguageProfile | None = None,
) -> EventDetection:
    before = max(0.0, float(getattr(args, "seconds_before", 0.0)))
    after = max(0.0, float(getattr(args, "seconds_after", 0.0)))
    context_rule = "default"
    molotov_before = _molotov_elim_before(args)
    language_code = getattr(args, "game_lang", DEFAULT_GAME_LANGUAGE)
    if language_code == AUTO_GAME_LANGUAGE:
        language_code = DEFAULT_GAME_LANGUAGE
    profile = profile or get_game_language_profile(language_code)
    if molotov_before > 0 and _is_molotov_elim_detection(detection, profile):
        before = max(before, molotov_before)
        context_rule = "molotov-elim-context"
    return replace(detection, context_rule=context_rule, clip_before_sec=before, clip_after_sec=after)


def _profile_clip(
    args: SimpleNamespace,
    idx: int,
    total: int,
    src: Path,
    status: str,
    detection: EventDetection,
    probe_seconds: float,
    trim_seconds: float,
    total_seconds: float,
) -> None:
    if not getattr(args, "profile", False):
        return
    print(
        "PROFILE "
        f"[{idx:02d}/{total}] {status} "
        f"target={detection.target or '-'} "
        f"probe={probe_seconds:.3f}s "
        f"detect={detection.detect_seconds:.3f}s "
        f"ocr_predict={detection.ocr_seconds:.3f}s "
        f"ocr_frame={detection.ocr_frame_seconds:.3f}s "
        f"samples={detection.sampled_count} "
        f"coarse={detection.ocr_coarse_frames} "
        f"refine={detection.ocr_refine_frames} "
        f"gate_skipped={detection.ocr_gate_skipped_frames} "
        f"trim={trim_seconds:.3f}s "
        f"total={total_seconds:.3f}s | {src.name}",
        flush=True,
    )


def _merge_detection_group(group: list[EventDetection]) -> EventDetection:
    first = group[0]
    last = group[-1]
    targets = [d.target for d in group if d.target]
    unique_targets = list(dict.fromkeys(targets))
    methods = list(dict.fromkeys(d.method for d in group if d.method))
    texts = list(dict.fromkeys(d.text for d in group if d.text))
    scores = [d.scores for d in group if d.scores]
    event_secs = [f"{d.event_sec:.3f}" for d in group if d.event_sec is not None]
    event_keys = [d.event_key for d in group if d.event_key]
    weapons = list(dict.fromkeys(d.event_weapon for d in group if d.event_weapon))
    context_rules = list(dict.fromkeys(d.context_rule for d in group if d.context_rule and d.context_rule != "default"))
    return EventDetection(
        first.duration_sec,
        first.event_sec,
        "+".join(methods) if methods else first.method,
        last.detector,
        " || ".join(texts),
        ";".join(scores),
        last.ocr_seconds,
        last.sampled_count,
        last.detect_seconds,
        last.ocr_frame_seconds,
        last.ocr_coarse_frames,
        last.ocr_refine_frames,
        unique_targets[0] if len(unique_targets) == 1 else "both",
        ";".join(event_keys),
        str(len(group)),
        ";".join(event_secs),
        ";".join(weapons),
        "+".join(context_rules) if context_rules else "default",
        max((_detection_before(d) for d in group), default=0.0),
        max((_detection_after(d) for d in group), default=0.0),
        last.ocr_gate_skipped_frames,
    )


def _merged_clip_plans(detections: list[EventDetection], before: float, after: float) -> list[tuple[EventDetection, float, float]]:
    valid = sorted((d for d in detections if d.event_sec is not None), key=lambda d: d.event_sec or 0.0)
    plans: list[tuple[EventDetection, float, float]] = []
    group: list[EventDetection] = []
    group_start = 0.0
    group_end = 0.0
    for detection in valid:
        event_before = _detection_before(detection) or before
        event_after = _detection_after(detection) or after
        start = max(0.0, (detection.event_sec or 0.0) - event_before)
        end = min(detection.duration_sec, (detection.event_sec or 0.0) + event_after)
        if end <= start:
            end = min(detection.duration_sec, start + 0.1)
        if group and start <= group_end + 1e-6:
            group.append(detection)
            group_end = max(group_end, end)
            continue
        if group:
            plans.append((_merge_detection_group(group), group_start, group_end))
        group = [detection]
        group_start = start
        group_end = end
    if group:
        plans.append((_merge_detection_group(group), group_start, group_end))
    return plans


def _clip_target_prefix(detection: EventDetection) -> str:
    suffix = f"x{detection.event_count}" if detection.event_count not in {"", "0", "1"} else ""
    return f"{detection.target or 'event'}{suffix}"


def _available_memory_gb() -> float | None:
    if os.name != "nt":
        return None

    class MemoryStatus(ctypes.Structure):
        _fields_ = [
            ("length", ctypes.c_ulong),
            ("memory_load", ctypes.c_ulong),
            ("total_physical", ctypes.c_ulonglong),
            ("available_physical", ctypes.c_ulonglong),
            ("total_page_file", ctypes.c_ulonglong),
            ("available_page_file", ctypes.c_ulonglong),
            ("total_virtual", ctypes.c_ulonglong),
            ("available_virtual", ctypes.c_ulonglong),
            ("available_extended_virtual", ctypes.c_ulonglong),
        ]

    status = MemoryStatus()
    status.length = ctypes.sizeof(MemoryStatus)
    if not ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
        return None
    return status.available_physical / (1024**3)


def _automatic_jobs(source_count: int, cpu_count: int | None = None, available_memory_gb: float | None = None) -> int:
    if source_count < 2:
        return 1
    cpu_count = cpu_count if cpu_count is not None else (os.cpu_count() or 1)
    available_memory_gb = available_memory_gb if available_memory_gb is not None else _available_memory_gb()
    if cpu_count < 8 or (available_memory_gb is not None and available_memory_gb < 12.0):
        return 1
    return 2


def _effective_jobs(requested_jobs: int | None, source_count: int) -> tuple[int, bool]:
    if requested_jobs is not None:
        return requested_jobs, False
    return _automatic_jobs(source_count), True


def _progress_line(phase: str, current: int, total: int, workers: int) -> str:
    payload = {"phase": phase, "current": current, "total": total, "workers": workers}
    return "PROGRESS " + json.dumps(payload, ensure_ascii=True, separators=(",", ":"))


def _scan_source_worker(
    src: Path,
    ffprobe: Path,
    language_code: str,
    candidate_times: list[float],
    config: OcrConfig,
) -> tuple[float, float, list[EventDetection]]:
    language = get_game_language_profile(language_code)
    suppress_worker_output = not getattr(sys, "frozen", False)
    backend = _WORKER_OCR_BACKENDS.get(language.paddle_lang)
    if backend is None:
        with suppress_process_output(suppress_worker_output):
            backend = load_backend(language)
        _WORKER_OCR_BACKENDS[language.paddle_lang] = backend
    cv2_module, ocr_engine = backend
    probe_started = time.time()
    duration = duration_sec(src, ffprobe)
    probe_seconds = time.time() - probe_started
    with suppress_process_output(suppress_worker_output):
        detections = detect_ocr_events(src, cv2_module, ocr_engine, duration, candidate_times, config)
    return duration, probe_seconds, detections


def run(args: SimpleNamespace) -> int:
    run_started = time.time()
    setup_started = time.time()
    input_paths, directory_input, single_file, base_folder = _validate_inputs(args)
    input_path = input_paths[0]
    ffmpeg, ffprobe = find_ffmpeg_pair(args.ffmpeg, args.ffprobe)
    files, file_profiles = _select_input_files(input_paths, args, directory_input)
    if not files:
        if _is_auto_game_language(args):
            hints = "; ".join(get_game_language_profile(code).source_file_hint for code in game_language_choices())
            raise SystemExit(f"No matching PUBG highlight source files found with automatic language detection. Tried: {hints}")
        language_profile = get_game_language_profile(_requested_game_language(args) or DEFAULT_GAME_LANGUAGE)
        raise SystemExit(
            f"No matching PUBG highlight source files found for game_lang={language_profile.code}: "
            f"{language_profile.source_file_hint}"
        )

    outdir, merged = _prepare_output_paths(args, input_path, files, base_folder, single_file)
    no_merge = _effective_no_merge(args, single_file)
    setup_seconds = time.time() - setup_started
    verbose = getattr(args, "verbose", False)
    if verbose or args.profile:
        print(f"windows_only=true", flush=True)
        print(f"sources={len(files)}", flush=True)
        print("detector=ocr", flush=True)
        print(f"game_lang={_requested_game_language(args)}", flush=True)
        print(f"detected_game_langs={_format_language_counts(file_profiles)}", flush=True)
        print(f"target={args.target}", flush=True)
        print(f"min_event_sec={_min_event_sec(args):.3f}", flush=True)
        print(f"molotov_elim_before={_molotov_elim_before(args):.3f}", flush=True)
        print(f"ffmpeg={ffmpeg}", flush=True)
        print(f"output_dir={outdir}", flush=True)
        print(f"merge_output={merged}", flush=True)
        print(f"merge={str(not no_merge).lower()}", flush=True)
        requested_jobs = getattr(args, "jobs", None)
        effective_jobs, automatic_jobs = _effective_jobs(requested_jobs, len(files))
        print(f"jobs={effective_jobs}{' (auto)' if automatic_jobs else ''}", flush=True)
    if args.profile:
        print(f"profile=true setup={setup_seconds:.3f}s", flush=True)
    candidate_csv = _candidate_csv_path(args, input_path, base_folder)
    candidates = read_candidate_csv(candidate_csv)
    no_full_scan = _use_fast_scan(args, candidate_csv)
    if verbose or args.profile:
        print(f"scan_mode={getattr(args, 'scan_mode', 'auto')}", flush=True)
        print(f"full_scan={str(not no_full_scan).lower()}", flush=True)
        print(f"candidate_csv={candidate_csv or ''}", flush=True)
    ocr_error = ""
    ocr_backends: dict[str, tuple[object, object]] = {}

    def backend_for(profile: GameLanguageProfile) -> tuple[object, object]:
        nonlocal ocr_error
        if profile.paddle_lang in ocr_backends:
            return ocr_backends[profile.paddle_lang]
        try:
            ocr_setup_started = time.time()
            with suppress_process_output(not verbose):
                backend = load_backend(profile, verbose=verbose)
            if args.profile:
                print(
                    f"PROFILE ocr_init={time.time() - ocr_setup_started:.3f}s lang={profile.paddle_lang}",
                    flush=True,
                )
        except OcrUnavailable as exc:
            ocr_error = str(exc)
            raise SystemExit(ocr_error) from exc
        ocr_backends[profile.paddle_lang] = backend
        return backend

    def ocr_config_for(profile: GameLanguageProfile) -> OcrConfig:
        return OcrConfig(
            target=args.target,
            language=profile,
            priority_window=args.priority_window,
            scan_start=args.scan_start,
            scan_end=args.scan_end,
            coarse_step=args.coarse_step,
            candidate_lookback=args.candidate_lookback,
            candidate_lookahead=args.candidate_lookahead,
            candidate_step=args.candidate_step,
            refine_before=args.refine_before,
            refine_after=args.refine_after,
            refine_step=args.refine_step,
            no_full_scan=no_full_scan,
            roi=args.roi,
            ocr_width=args.ocr_width,
            brightness_gate=not getattr(args, "no_brightness_gate", False),
        )

    records: list[dict[str, str]] = []
    clips: list[Path] = []
    methods: Counter[str] = Counter()
    detectors: Counter[str] = Counter()
    encoders: Counter[str] = Counter()
    min_event_sec = _min_event_sec(args)
    jobs, _ = _effective_jobs(getattr(args, "jobs", None), len(files))
    print(_progress_line("scan", 0, len(files), jobs), flush=True)
    parallel_results: dict[Path, tuple[float, float, list[EventDetection]]] = {}
    if jobs > 1:
        with ProcessPoolExecutor(max_workers=jobs) as executor:
            futures = {
                executor.submit(
                    _scan_source_worker,
                    src,
                    ffprobe,
                    file_profiles[src].code,
                    candidates.get(src.name, []),
                    _ocr_config_for_source(ocr_config_for(file_profiles[src]), src, file_profiles[src]),
                ): src
                for src in files
            }
            for completed, future in enumerate(as_completed(futures), 1):
                src = futures[future]
                parallel_results[src] = future.result()
                print(_progress_line("scan", completed, len(files), jobs), flush=True)

    for idx, src in enumerate(files, 1):
        language_profile = file_profiles[src]
        ocr_config = ocr_config_for(language_profile)
        clip_started = time.time()
        detect_started = time.time()
        source_ocr_config = _ocr_config_for_source(ocr_config, src, language_profile)
        if source_ocr_config.no_full_scan != ocr_config.no_full_scan:
            print(f"[{idx:02d}/{len(files)}] full_scan=true multi-kill-source | {src.name}", flush=True)
        if jobs > 1:
            dur, probe_seconds, detections = parallel_results[src]
        else:
            cv2_module, ocr_engine = backend_for(language_profile)
            probe_started = time.time()
            dur = duration_sec(src, ffprobe)
            probe_seconds = time.time() - probe_started
            with suppress_process_output(not verbose):
                detections = detect_ocr_events(src, cv2_module, ocr_engine, dur, candidates.get(src.name, []), source_ocr_config)
        if len(detections) == 1 and detections[0].event_sec is None:
            detection = detections[0]
            if detection.detect_seconds == 0.0:
                detection.detect_seconds = time.time() - detect_started
            methods[detection.method] += 1
            detectors[detection.detector] += 1
            total_seconds = time.time() - clip_started
            print(
                f"[{idx:02d}/{len(files)}] SKIP {detection.target} {detection.method} "
                f"lang={language_profile.code} | {src.name} | {detection.text[:80]}",
                flush=True,
            )
            _profile_clip(args, idx, len(files), src, "SKIP", detection, probe_seconds, 0.0, total_seconds)
            records.append(_record_skip(idx, src, detection, probe_seconds, 0.0, total_seconds))
            continue

        detections, early_skips = _partition_too_early_detections(detections, min_event_sec)
        for detection in early_skips:
            if detection.detect_seconds == 0.0:
                detection.detect_seconds = time.time() - detect_started
            methods[detection.method] += 1
            detectors[detection.detector] += 1
            total_seconds = time.time() - clip_started
            print(
                f"[{idx:02d}/{len(files)}] SKIP {detection.target} "
                f"{detection.event_secs or f'{detection.event_sec:.3f}'}s < {min_event_sec:.3f}s "
                f"{detection.method} lang={language_profile.code} | {src.name}",
                flush=True,
            )
            _profile_clip(args, idx, len(files), src, "SKIP", detection, probe_seconds, 0.0, total_seconds)
            records.append(_record_skip(idx, src, detection, probe_seconds, 0.0, total_seconds))
        if not detections:
            continue
        detections = [_apply_context_rules(detection, args, language_profile) for detection in detections]

        for detection, start, end in _merged_clip_plans(detections, args.seconds_before, args.seconds_after):
            if detection.detect_seconds == 0.0:
                detection.detect_seconds = time.time() - detect_started

            output = ""
            encoder = ""
            trim_seconds = 0.0
            if not args.dry_run:
                output_path = unique_path(outdir / f"{len(clips) + 1:03d}_{_clip_target_prefix(detection)}_{src.name}")
                trim_started = time.time()
                encoder = trim_clip(src, output_path, start, end - start, ffmpeg)
                trim_seconds = time.time() - trim_started
                encoders[encoder] += 1
                clips.append(output_path)
                output = str(output_path)

            methods[detection.method] += 1
            detectors[detection.detector] += 1
            total_seconds = time.time() - clip_started
            print(
                f"[{idx:02d}/{len(files)}] INCLUDE {detection.target} {detection.event_secs or f'{detection.event_sec:.3f}'}s "
                f"{start:.3f}-{end:.3f} {detection.method} lang={language_profile.code} | {src.name}",
                flush=True,
            )
            _profile_clip(args, idx, len(files), src, "INCLUDE", detection, probe_seconds, trim_seconds, total_seconds)
            records.append(
                _record_detection(
                    idx,
                    src,
                    detection,
                    start,
                    end,
                    encoder,
                    output,
                    probe_seconds,
                    trim_seconds,
                    total_seconds,
                )
            )

    csv_path = outdir / "检测与裁剪记录.csv"
    with csv_path.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(records[0].keys()))
        writer.writeheader()
        writer.writerows(records)

    concat_list = None
    merge_duration = 0.0
    merge_size = 0.0
    if clips and not no_merge and not args.dry_run:
        merge_started = time.time()
        concat_list = concat_clips(clips, merged, ffmpeg)
        merge_duration = duration_sec(merged, ffprobe)
        merge_size = merged.stat().st_size / 1024 / 1024
        if args.profile:
            print(f"PROFILE merge={time.time() - merge_started:.3f}s clips={len(clips)}", flush=True)

    included_count = sum(1 for row in records if row["Status"] == "included")
    summary = {
        "source_count": len(files),
        "included_count": included_count,
        "skipped_count": sum(1 for row in records if row["Status"] == "skipped"),
        "dry_run": args.dry_run,
        "detector": "ocr",
        "game_lang": _requested_game_language(args),
        "detected_game_langs": dict(Counter(profile.code for profile in file_profiles.values())),
        "min_event_sec": min_event_sec,
        "molotov_elim_before": _molotov_elim_before(args),
        "ocr_unavailable_reason": ocr_error,
        "output_dir": str(outdir),
        "merge": not no_merge,
        "merge_output": "" if args.dry_run or no_merge or not clips else str(merged),
        "scan_mode": getattr(args, "scan_mode", "auto"),
        "full_scan": not no_full_scan,
        "candidate_csv": "" if candidate_csv is None else str(candidate_csv),
        "concat_list": "" if concat_list is None else str(concat_list),
        "csv": str(csv_path),
        "merge_duration_sec": round(merge_duration, 3),
        "merge_size_mb": round(merge_size, 1),
        "methods": dict(methods),
        "detectors": dict(detectors),
        "encoders": dict(encoders),
        "profile_total_sec": round(time.time() - run_started, 3),
    }
    summary_path = outdir / "summary.json"
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print("SUMMARY " + json.dumps(summary, ensure_ascii=False), flush=True)

    if included_count == 0:
        return 2
    return 0
