# Profile schema v2 (#87)

`profile.json` is written for every successful application run (including exit 2,
no included clips). `--profile` additionally emits one final
`PROFILE {"run_id":...,"stages":...}` record. Its `stages` object is identical
to the file, not independently measured. Existing per-source PROFILE lines and
CSV fields remain diagnostic/task measurements, not stage totals.

## Wall-clock stages

All values are seconds from one monotonic run clock, rounded to milliseconds.
Only one phase owns each interval; `total` covers the run through the reporting
freeze boundary. The sum of the eight stages equals total before rounding
(up to 0.005 seconds discrepancy after independent rounding).

- `setup`: input/output planning, tools/settings discovery, candidates, and
  best-effort version/codec diagnostics. No OCR execution.
- `scan`: source configuration, cache lookup, OCR worker initialization and
  execution, cache writes, and completion collection. Ends before filtering or
  cross-source verification. A fully cached run still has scan wall time.
- `planning`: preparation/minimum-event filtering **before** dedupe plus final
  grouping, stable numbering and destination planning **after** dedupe. These
  two disjoint intervals accumulate; neither contains dedupe time. Output
  directory preparation (still after successful scans) is included here.
- `dedupe`: cross-source duplicate ranking and optional frame verification.
- `trim`: the unchanged serial execution loop, including record/decision
  bookkeeping, reservation, encoding, strict output probe and atomic publication.
  Dry runs still perform the loop but execute no media writes.
- `merge`: merge orchestration, reservation, concatenation, output probe,
  concat-list and media publication. **Not** merged media duration.
- `archive`: existing conditional source archival after successful media work.
- `reporting`: CSV writing before merge (preserving its failure gate and never
  archiving after CSV failure), plus final profile/summary object preparation.
  These disjoint intervals accumulate like planning.

The explicit freeze is **before final timing JSON serialization, file writes and
stdout emission**. These final emissions (including summary serialization) are
excluded from both reporting and total; timing one's own final serialization
recursively is impossible. CSV writing and ordinary report preparation are
included. `summary.profile_total_sec` uses this same frozen total. Failed or
cancelled runs retain existing error/cleanup behavior; no partial success profile
is promised.

## Nested measurements and work

`aggregates.*_seconds_sum` contains nested task measurements, not extra phases:
cache reads, cache writes and Trim task work. `trim_tasks` records successful
published tasks by stable `sequence`, with `encode_seconds` (all production
attempts including fallback), `output_probe_seconds` (strict FFprobe duration
validation), `publication_seconds` (sync/atomic publish and owned staging cleanup),
and `total_seconds` (reservation through publication). The three operations do
not overlap; total contains them plus reservation/bookkeeping, and must **not**
be added to them. Corresponding aggregates are `trim_encode_seconds_sum`,
`trim_output_probe_seconds_sum`, `trim_publication_seconds_sum`, and
`trim_task_total_seconds_sum`, summed before rounding. They are nested work,
not additional wall-clock phases. The encoding sum preserves the
historical CSV `TrimSeconds` definition (FFmpeg call, excluding reservation,
validation and publication); it is not a replacement for `stages.trim`.
`summary.merge_duration_sec` remains media duration and retains its old meaning.
No `sampling` or `candidate_generation` alias duplicates `scan`.

`telemetry.current` contains **completed scan** measurements from
`ScanReport.stats` and its final `work` totals, not detection snapshots. It
includes work after the last accepted event: requests, successes, scanner cache
hits/skips, actual refine/assist calls and their prediction time, plus final
prediction/frame/detection time totals. Detection fields and CSV retain their
historical event-time snapshot semantics unchanged.

`telemetry.cached_historical` contains only the maximum detection snapshot per
source from the unchanged cache payload. For an event-bearing source, this
snapshot stops at the last accepted event and **is not whole-source work**; later
OCR requests and time cannot be reconstructed. A no-event detection is taken at
scan completion. Historical `refine_frames_snapshot` and
`assist_budget_used_snapshot` retain the available frame/budget evidence and are
not mislabeled as true refine/assist call counts. The explicit `current_basis`
and `cached_historical_basis` fields identify these different measurement bases.

Each source index contributes once before filtering/dedupe/grouping. Different
directories with identical basenames remain distinct. `*_seconds_sum` is
aggregated work and may overlap other nested measurements or exceed wall time
with multiple OCR workers. Request/cache-hit counters describe scanner work,
not persistent cache hits. All current duration
probes, including probes needed for empty cached detection lists, belong to
`current.probe_seconds_sum`; the historical bucket does not invent old probe time.

The scanner does not return partial runtime telemetry after an unreadable
source failure. `sources_without_detection_telemetry` exposes missing completed
measurements in the current bucket or missing snapshots in the historical bucket;
zero request totals for such sources **do not prove zero attempted OCR work**.
Empty detection lists also provide no detection metrics. No historical telemetry
is presented as current OCR work.

`dedupe_work` reports identity-bearing candidates, cross-source pair comparisons,
verification windows attempted, verification errors and proven duplicates.
A short-window rejection can count a comparison with no verification window;
conservative missing/mismatching-frame rejections are not decoder errors.

## Runtime metadata and compatibility

`runtime` records OCR worker capacity and the launched current OCR worker pool
(`ocr_workers_active`, zero on all-cache-hit runs), effective per-worker OCR
threads, serial `trim_workers: 1`, provider mode, strict validation, scan/gate
settings, dry-run and per-source effective scanner configs keyed only by source
index. Cache enabled/hits/misses remain under `cache`. `source_codec_counts`
counts inputs (including cached ones); failed diagnostic probes count as
`unknown` and cannot change source readability classification.

`ffmpeg_version` contains only the validated version token from the first version
line, never build flags or executable paths; unavailable/unsafe output is null.
The extra diagnostic probes are bounded, best-effort, silent, and charged to
setup. No new private paths are stored by these fields. Existing path-bearing
report fields retain their compatibility contract.

`ocr_providers` remains compatible with the previous report/summary array.
Consumers investigating performance should use `ocr_providers_current` versus
`ocr_providers_cached_historical`: `provider_actual` and fallback reasons in a
cache hit describe the **historical** execution, not provider execution now.
Legacy entries without provider data stay unknown rather than implying a new
session ran. CSV columns, candidate schema, summary fields, cache payloads,
ordering, output safety and scan/decode/strict-validation policies are unchanged.

## Trim encoder capability (#89)

`runtime.trim_validation: "ffprobe_duration_strict"` explicitly names the existing
validation policy; `runtime.validation: "strict"` remains compatible. FFprobe is
not removed or made optional. `runtime.trim_encoder` records `capability`
(`not_probed`, `available`, `unavailable`), `probe_seconds`, `fallback_reason`
(null, `capability_probe_failed`, `production_nvenc_failed`) and
`production_fallbacks`. Reasons never contain native stderr or private paths.

Before the first real Trim task, one bounded (15-second) FFmpeg child encodes a
generated 320×240 source into owned staged MP4 using the production h264_nvenc
p4/vbr/cq22/b:v0 options and faststart. Staging is removed on success, failure,
timeout and cancellation. No private source is needed. The result is cached in
that run's encoder instance, not globally; dry-run/no-clips remain `not_probed`.
Failed capability selects libx264 without retrying NVENC for every clip. A
successful capability retains production NVENC-first and software fallback on
real production failures. Cancellation is propagated, never interpreted as
unavailable capability or an invitation to retry software.

Capability time is included in wall-clock `stages.trim`, but excluded from every
clip's encode/total and CSV `TrimSeconds`; it appears **only** in the separate
`runtime.trim_encoder.probe_seconds` nested measurement. Task totals exclude
capability discovery; stage Trim also includes orchestration/decision work.
