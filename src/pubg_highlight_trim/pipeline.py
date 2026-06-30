from __future__ import annotations

import csv
import json
import shutil
from collections import Counter
from pathlib import Path
from types import SimpleNamespace

from . import healthbar
from .ffmpeg_tools import concat_clips, duration_sec, find_ffmpeg_pair, trim_clip
from .models import EventDetection
from .ocr import OcrConfig, OcrUnavailable, detect_event as detect_ocr_event, load_backend
from .source_files import iter_source_files


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


def _prepare_output_paths(args: SimpleNamespace, input_path: Path, base_folder: Path, single_file: bool) -> tuple[Path, Path]:
    if single_file:
        outdir = args.output_dir or base_folder / f"{input_path.stem}_pubg_trim_clips"
        final = args.final or base_folder / f"{input_path.stem}_pubg_trim.mp4"
    else:
        outdir = args.output_dir or base_folder / "pubg_highlight_trim_output"
        final = args.final or base_folder / "pubg_highlight_trim_montage.mp4"

    if args.overwrite:
        if outdir.exists():
            shutil.rmtree(outdir)
        if final.exists():
            final.unlink()
    else:
        outdir = unique_dir(outdir)
        final = unique_path(final)

    outdir.mkdir(parents=True, exist_ok=True)
    final.parent.mkdir(parents=True, exist_ok=True)
    return outdir, final


def _blank_record(idx: int, src: Path, dur: float, status: str, method: str, detector: str) -> dict[str, str]:
    return {
        "Index": str(idx),
        "Name": src.name,
        "DurationSec": f"{dur:.3f}",
        "Status": status,
        "EventSec": "",
        "KeepStartSec": "",
        "KeepEndSec": "",
        "KeepDurationSec": "",
        "Method": method,
        "Detector": detector,
        "PaddleText": "",
        "PaddleScores": "",
        "OpeningRedRatio": "",
        "OcrSeconds": "0.000",
        "SampledFrames": "0",
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
    opening_red_ratio: float,
) -> dict[str, str]:
    return {
        "Index": str(idx),
        "Name": src.name,
        "DurationSec": f"{detection.duration_sec:.3f}",
        "Status": "included",
        "EventSec": f"{detection.event_sec:.3f}" if detection.event_sec is not None else "",
        "KeepStartSec": f"{start:.3f}",
        "KeepEndSec": f"{end:.3f}",
        "KeepDurationSec": f"{end - start:.3f}",
        "Method": detection.method,
        "Detector": detection.detector,
        "PaddleText": detection.text,
        "PaddleScores": detection.scores,
        "OpeningRedRatio": f"{opening_red_ratio:.3f}",
        "OcrSeconds": f"{detection.ocr_seconds:.3f}",
        "SampledFrames": str(detection.sampled_count),
        "Encoder": encoder,
        "Output": output,
    }


def _record_skip(idx: int, src: Path, detection: EventDetection, opening_red_ratio: float) -> dict[str, str]:
    row = _blank_record(idx, src, detection.duration_sec, "skipped", detection.method, detection.detector)
    row.update(
        {
            "PaddleText": detection.text,
            "PaddleScores": detection.scores,
            "OpeningRedRatio": f"{opening_red_ratio:.3f}",
            "OcrSeconds": f"{detection.ocr_seconds:.3f}",
            "SampledFrames": str(detection.sampled_count),
        }
    )
    return row


def run(args: SimpleNamespace) -> int:
    input_path = args.input.resolve()
    if not input_path.exists():
        raise SystemExit(f"Input path does not exist: {input_path}")

    ffmpeg, ffprobe = find_ffmpeg_pair(args.ffmpeg, args.ffprobe)
    single_file = input_path.is_file()
    base_folder = input_path.parent if single_file else input_path
    if single_file:
        if input_path.suffix.lower() != ".mp4":
            raise SystemExit(f"Single-file input must be an .mp4: {input_path}")
        files = [input_path]
    elif input_path.is_dir():
        files = iter_source_files(input_path, args.include_view_replays, args.recursive)
    else:
        raise SystemExit(f"Input path is neither a file nor a directory: {input_path}")
    if not files:
        raise SystemExit("No matching .被击倒.DVR*.mp4 or .淘汰.DVR*.mp4 source files found")

    outdir, final = _prepare_output_paths(args, input_path, base_folder, single_file)
    print(f"windows_only=true", flush=True)
    print(f"sources={len(files)}", flush=True)
    print(f"detector={args.detector}", flush=True)
    print(f"ffmpeg={ffmpeg}", flush=True)
    print(f"output_dir={outdir}", flush=True)
    print(f"final={final}", flush=True)

    candidates = read_candidate_csv(args.candidate_csv)
    ocr_backend: tuple[object, object] | None = None
    ocr_error = ""
    if args.detector in {"auto", "ocr"}:
        try:
            ocr_backend = load_backend()
        except OcrUnavailable as exc:
            ocr_error = str(exc)
            if args.detector == "ocr":
                raise SystemExit(ocr_error) from exc
            print(f"OCR unavailable; falling back to health detector: {ocr_error}", flush=True)

    ocr_config = OcrConfig(
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
        no_full_scan=args.no_full_scan,
        roi=args.roi,
        ocr_width=args.ocr_width,
    )

    records: list[dict[str, str]] = []
    clips: list[Path] = []
    methods: Counter[str] = Counter()
    detectors: Counter[str] = Counter()
    encoders: Counter[str] = Counter()

    for idx, src in enumerate(files, 1):
        dur = duration_sec(src, ffprobe)
        opening_red_ratio = 0.0
        opening_samples = 0
        if not args.allow_starts_downed:
            starts_downed, opening_red_ratio, opening_samples = healthbar.opening_already_downed(
                src,
                ffmpeg,
                check_start=args.opening_check_start,
                check_end=min(args.opening_check_end, dur),
                fps=args.opening_check_fps,
                red_threshold=args.opening_red_threshold,
            )
            if starts_downed:
                detection = EventDetection(
                    dur,
                    None,
                    "skipped-starts-already-downed",
                    "opening-health-check",
                    sampled_count=opening_samples,
                )
                methods[detection.method] += 1
                detectors[detection.detector] += 1
                print(f"[{idx:02d}/{len(files)}] SKIP {detection.method} opening_red={opening_red_ratio:.3f} | {src.name}", flush=True)
                records.append(_record_skip(idx, src, detection, opening_red_ratio))
                continue

        detection: EventDetection | None = None
        if args.detector in {"auto", "ocr"} and ocr_backend is not None:
            cv2_module, ocr_engine = ocr_backend
            detection = detect_ocr_event(src, cv2_module, ocr_engine, dur, candidates.get(src.name, []), ocr_config)
            if detection.event_sec is None and args.detector == "auto":
                print(f"[{idx:02d}/{len(files)}] OCR MISS {detection.method}; trying health fallback | {src.name}", flush=True)
                health_detection = healthbar.detect_event(src, ffmpeg, ffprobe)
                if health_detection.event_sec is not None:
                    health_detection.method = f"health-fallback:{health_detection.method}"
                    detection = health_detection
        if detection is None or (detection.event_sec is None and args.detector == "health"):
            detection = healthbar.detect_event(src, ffmpeg, ffprobe)
        if detection.event_sec is None and args.detector == "auto" and ocr_backend is None:
            detection = healthbar.detect_event(src, ffmpeg, ffprobe)

        if detection.event_sec is None:
            methods[detection.method] += 1
            detectors[detection.detector] += 1
            print(f"[{idx:02d}/{len(files)}] SKIP {detection.method} | {src.name} | {detection.text[:80]}", flush=True)
            records.append(_record_skip(idx, src, detection, opening_red_ratio))
            continue

        start = max(0.0, detection.event_sec - args.seconds_before)
        end = min(detection.duration_sec, detection.event_sec + args.seconds_after)
        if end <= start:
            end = min(detection.duration_sec, start + 0.1)

        output = ""
        encoder = ""
        if not args.dry_run:
            output_path = unique_path(outdir / f"{len(clips) + 1:03d}_{src.name}")
            encoder = trim_clip(src, output_path, start, end - start, ffmpeg)
            encoders[encoder] += 1
            clips.append(output_path)
            output = str(output_path)

        methods[detection.method] += 1
        detectors[detection.detector] += 1
        print(
            f"[{idx:02d}/{len(files)}] INCLUDE {detection.event_sec:.3f}s {start:.3f}-{end:.3f} {detection.method} | {src.name}",
            flush=True,
        )
        records.append(_record_detection(idx, src, detection, start, end, encoder, output, opening_red_ratio))

    csv_path = outdir / "检测与裁剪记录.csv"
    with csv_path.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(records[0].keys()))
        writer.writeheader()
        writer.writerows(records)

    concat_list = None
    final_duration = 0.0
    final_size = 0.0
    if clips and not args.no_merge and not args.dry_run:
        concat_list = concat_clips(clips, final, ffmpeg)
        final_duration = duration_sec(final, ffprobe)
        final_size = final.stat().st_size / 1024 / 1024

    included_count = sum(1 for row in records if row["Status"] == "included")
    summary = {
        "source_count": len(files),
        "included_count": included_count,
        "skipped_count": sum(1 for row in records if row["Status"] == "skipped"),
        "dry_run": args.dry_run,
        "detector": args.detector,
        "ocr_unavailable_reason": ocr_error,
        "output_dir": str(outdir),
        "final": "" if args.dry_run or args.no_merge or not clips else str(final),
        "concat_list": "" if concat_list is None else str(concat_list),
        "csv": str(csv_path),
        "final_duration_sec": round(final_duration, 3),
        "final_size_mb": round(final_size, 1),
        "methods": dict(methods),
        "detectors": dict(detectors),
        "encoders": dict(encoders),
    }
    summary_path = outdir / "summary.json"
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print("SUMMARY " + json.dumps(summary, ensure_ascii=False), flush=True)

    if included_count == 0:
        return 2
    return 0
