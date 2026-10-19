# Rust Windows CI: cache and assurance boundaries

Issue: #76. Application/test semantics are unchanged.

## Gates

`rust-quality` and `windows-assurance` run independently on Windows. Quality owns formatting, all-feature unit/real-media tests, the highlight planner contract and optional-feature checks. Assurance owns the **only** release build, all pinned-model/ONNX/CPU/ROI/CLI acceptance and the complete Windows bundle/license/supply-chain checks. The existing `rust-and-windows-assurance` check is a final AND gate so existing branch protection cannot pass while one of the new jobs fails or is skipped.

Trade-off: on the first FFmpeg cache miss, both independent jobs download the same pinned archive; on hits both restore/extract it. This does not provide single-download cross-job FFmpeg reuse. A shared preparation job would add a serial startup/artifact-transfer seam to both gates. FFmpeg is reused within assurance by bundling its verified extraction. Keep this limitation visible when comparing cold performance.

There is deliberately no cross-job `target`/model/binary transfer: both jobs need debug dependencies with different features, but only assurance needs models and release outputs. Copying a large target artifact to the quality job would serialize the jobs and add transfer cost. Cargo dependency caches are separated by job role. No assurance command is removed, and isolated CLI tests retain `--test-threads=1`.

PR runs cancel obsolete PR runs. Main pushes do not cancel earlier main runs. Changes to `vendor/ppocr-rs/**` and the local setup action trigger both gates.

## Cache contracts

- `Swatinem/rust-cache@v2` caches Cargo registry/git and `rust/target` dependencies. Its automatic key includes runner OS/architecture, rustc version, Cargo manifests/lockfile/config and compiler environment. The explicit key also includes feature/profile role, workflow/setup/release-script and vendored ppocr sources. Application workspace outputs remain Cargo's responsibility; a restored target never substitutes for running Cargo.
- FFmpeg uses the dated release, exact archive name and SHA-256 in `scripts/ffmpeg-lock.json`. Only `vendor/ffmpeg-cache` is cached. Every invocation verifies the archive before extraction; executables/DLLs are freshly extracted and executable version checks remain required. A cache miss downloads and verifies the same pinned source. Do not replace this with a floating `latest` cache. The selected BtbN month-end release is retained upstream for two years (not the daily build's 14 days); renew this pin before September 2028, or mirror the exact checksum-verified archive to maintainer-controlled storage. Cache eviction must not be the trigger for discovering expiry. To update, verify the upstream checksum, update the lock, and run both gates.
- OCR model cache keys hash the pinned model descriptor. `xtask download` checks existing SHA-256 and downloads only missing/mismatched assets; `xtask verify --check-onnx` still checks all model/dictionary contracts. A corrupt asset cannot pass merely because the cache action reported a hit.
- GitHub caches are immutable and subject to GitHub quota/LRU eviction. No run-ID keys create an unbounded cache per commit. Changed toolchains/configurations get new keys. Cache eviction is a performance event, not a correctness dependency. Never execute downloaded asset contents before checksum verification.

## Same-run reuse

```powershell
./scripts/build_rust_release.ps1
# Use rust/target/release/xtask.exe directly for model assurance.
./scripts/build_windows.ps1 -ModelDirectory "$env:RUNNER_TEMP\ocr_models" -ReleaseDirectory "rust\target\release"
```

The shared release command builds app, engine and xtask together with one canonical DirectML delay-load flag pair and `--timings`, then restores the caller's flags. A release manifest binds three binary hashes to HEAD, toolchain and build configuration. Reuse validates this manifest before executing xtask. It detects accidental corruption/incompatible artifacts, not malicious modifications by a trusted build producer; it is not a signature or provenance attestation.

Model files are copied into the bundle and verified again, including ONNX against the source engine. Packaging still runs audit, licenses, supply-chain manifest, Windows smoke and size checks. Standalone `build_windows.ps1` with no reuse arguments still builds and downloads itself.

## Timing and reproducibility evidence

Baseline successful run [39187027404](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39187027404) (GitHub job/step timestamps):

| Phase | Duration |
| --- | ---: |
| Entire original job | 24m18s |
| FFmpeg preparation | 1m34s |
| All-feature tests | 3m15s |
| Release app/engine build | 2m50s |
| Model preparation/verification | 2m54s |
| CLI fault/cache/provider/cancellation acceptance | 8m27s |
| Bundle assurance | 1m02s |

Measured hosted results for commit `2eb4837` ([run 39196575725](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39196575725)):

| Phase | Cold attempt 1 | Warm attempt 2 |
| --- | ---: | ---: |
| Quality job | 6m02s (pass) | 4m45s (fixture collision failure) |
| Full Windows assurance | 23m25s (pass) | 12m15s (pass) |
| Release build | 5m14s | 54s |
| CLI acceptance | 10m14s | 7m06s |
| Bundle assurance | 52s | 49s |

Attempt 1 logs show both Cargo roles and FFmpeg/models missed; attempt 2 logs show all hit and model SHA-256 reuse, with full ONNX verification and bundle checks passing. Warm complete assurance reached the target; cold complete assurance did not. Attempt 2 is **not** a successful overall run: the unchanged parallel `tools_parity` tests collided on a timestamp-only temporary path (dictionary contents matched the other test's fixture). The follow-up test-only fix adds a process-local atomic counter without changing assertions, scheduling, or application semantics. The PR must also link a successful full run of that fix before merging.

Use `gh run view <id> --json jobs` for authoritative per-step `startedAt` / `completedAt` (cache restore, assets, build/test, inference, packaging). Job summaries report each cache hit/miss and reuse mode. The critical path is the slower independent job, plus the final gate and queue time; do not sum parallel job durations or describe quality alone as full assurance.

Manual dispatch exposes `cold_cache=true` to bypass **all** cache restores/saves; `cargo_timings=true` uploads the release Cargo HTML reports for seven days. Ordinary PR runs generate build timings but do not upload them. The first PR run with previously unseen keys is also cold; rerunning the same commit after a successful save supplies warm-cache evidence. Check the logs rather than inferring a hit from the run number.

Validation checklist before claiming the 10–15 minute target:

1. Validate YAML/actions with actionlint and parse changed PowerShell scripts.
2. Run both jobs on a clean hosted runner; verify every original acceptance and bundle check passes.
3. Rerun the same commit with caches enabled; record restored Cargo/model/FFmpeg keys and per-phase elapsed times.
4. Confirm model/binary tampering fails validation, lock/config changes invalidate the relevant key, and an empty cache remains reproducible.
5. Record measured cold/warm run links and durations in the PR. Do not claim speedup until successful runs exist. If full assurance exceeds the target, report its actual duration separately from quality feedback.
