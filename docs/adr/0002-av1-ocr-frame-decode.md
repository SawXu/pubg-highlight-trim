# ADR-0002: AV1 OCR frame reads stay on per-seek software decode

## Context

Ticket #52 asked to bring AV1 OCR scan time near H.264 levels via hardware
decode (`-hwaccel` / `*_cuvid`) and/or a persistent single-pass sequential
decoder replacing per-read `-ss` spawns.

Measured on the regression corpus (`F:/NVIDIA/TEMP/Highlights/...`, 2560×1440
@60, AV1 source vs same-named H.264 re-encode, 19-read ascending + backward
refine schedule; `scripts/bench_frame_read.py`):

| variant | AV1 ms/read | H.264 ms/read |
|---|---|---|
| software (libdav1d / h264) | ~145 | ~393 |
| `-hwaccel cuda` | ~223 | ~407 |
| `-hwaccel d3d11va` | ~325 | ~526 |
| `-hwaccel d3d12va` | ~349 | ~554 |
| `*_cuvid` decoder | ~191 | ~335 |

Per-spread breakdown: process spawn ~30 ms, so seek+decode dominates.
Hardware paths *lose* because every spawn pays GPU context init +
hwdownload. A sequential decoder loses worse: coarse-scan gaps are ~4 s
(~240 frames at 60 fps) and software AV1 sustains only ~330 fps of sequential
decode — plus ~11 MB/frame rawvideo pipe bandwidth — versus ~150 ms for a
fresh `-ss` spawn.

Meanwhile libdav1d already beats H.264 software decode on this pattern
(~0.4×), because the H.264 re-encodes carry long GOPs and accurate `-ss`
seek must decode forward from the previous keyframe.

## Decision

- Production default: per-`read_at` `ffmpeg -ss` spawn, software decode
  (`DEFAULT_FFMPEG_HWACCEL = None` in `pipeline.py`).
- `FfmpegFrameSource.hwaccel` remains opt-in via
  `PUBG_HIGHLIGHT_TRIM_FFMPEG_HWACCEL`; first hardware-path failure
  permanently falls back to software, so a missing/incompatible GPU can
  never produce `VideoUnreadable` or wrong pixels for a decodable file.
- No persistent decoder — it does not win at this access density. Revisit if
  the scan pattern becomes dense (e.g. sub-second sampling throughout).

## Consequences

- Ticket acceptance (AV1 ≤ ~1.5× H.264) is met without a decode-path change.
- `--profile` already exposes `ocr_frame=` seconds per clip for future
  verification.
- Hardware-decoded frames differ from software output by small rounding
  deltas (observed max |Δ| 44 of 255 on single pixels); acceptable for OCR,
  but only when hwaccel is explicitly enabled.
