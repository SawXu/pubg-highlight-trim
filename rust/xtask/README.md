# Rust assurance tools

Run from the repository root with `cargo run --manifest-path rust/Cargo.toml -p xtask -- <command>`.

## Replacements for the final four Python tools

- `build-real-roi --record <CSV> [--record <CSV> ...] --media-root <local directory> --output <empty directory> --ffmpeg <executable> [--limit-per-language 4]`: reads UTF-8/BOM CSV; selects included, nonempty PaddleText rows by the historical language heuristic and self-death/own-kill split, prioritizing assist/delayed-elimination. Recursively resolves source basenames, preferring an `original` directory; seeks read-only using FFmpeg and crops `[.30,.66,.70,.75]`, truncating pixel coordinates. Writes JPEG quality 95, `baseline.jsonl` and `manifest.json`. No upload capability. Missing/unreadable samples are skipped; empty results fail. Unlike the old script, existing nonempty output is rejected to prevent overwrite. FFmpeg replaces OpenCV decoding; JPEG bytes need not be identical.
- `frame-bench --ffmpeg <exe> --ffprobe <exe> --av1 <file> --h264 <file> [--repeat 2]`: explicit local A/B inputs replace automatic corpus discovery. Retains coarse range `range(4,int(duration)-2,4)`, backward/bisection refine and forward assist reads, one unmeasured warmup per variant, and the fastest complete repeat by total latency. Reports `n`, `bad`, total/mean/median/p90 milliseconds and schedule for all historical software/hardware variants. Unsupported hardware is counted as bad, not silently replaced by software.
- `verify --model-dir <directory> --manifest <JSON> --check-onnx --engine <source-built ocr_engine executable> [--timeout-secs 120]`: hashes all assets, validates dictionary UTF-8 and at least 100 **distinct** symbols, checks protobuf IO contract, then requires full engine loading and actual detector/recognizer inference. Protobuf name checks alone are not a successful `--check-onnx`. No Python, binding compiler, or download is used. The supplied engine must implement the existing Rust JSONL/engine-init protocol and must be built from trusted project source.
- `onnx-smoke --model-dir <directory> --engine <source-built executable> [--timeout-secs 120]`: uses the existing repository `real/frame_001.jpg` ROI, compares frozen normalized OCR text and canonical event identity, requires nonempty numeric boxes and CPU engine-init success. This forces recognition after detection; a black frame/status-only response cannot pass. Both sessions are eagerly loaded by the existing engine. Concurrent pipe draining avoids request/response deadlock; timeout kills/reaps the child.

The strict 18-frame `real-roi` command remains separate and unchanged: it uses the audited hash-bound `reference.json` and canonical event extraction; semantic mismatches remain failures.

## Release license inventory (required before final manifest)

```powershell
cargo run --locked --manifest-path <repo>/rust/Cargo.toml -p xtask -- licenses <bundle>
cargo run --locked --manifest-path <repo>/rust/Cargo.toml -p xtask -- manifest <bundle>
```

Run `licenses` after assembling the release and before generating its final manifest/zip; for a WPF bundle pass the bundled CLI directory, then generate the outer bundle manifest last. The command invokes `cargo metadata --locked --filter-platform x86_64-pc-windows-msvc --features cli,directml`, starting only at `pubg-ocr-engine`, traversing normal/build edges and excluding dev/xtask-only dependencies. Proc-macros and build dependency subgraphs are `build-only`; ordinary application dependencies are `shipped` (potentially linked/optimized out, **not** asserted to be separate artifact files). Inventory contains stable name/version/source/SPDX metadata, dependency edges, lockfile hash, and copied-notice hashes. `--manifest-path` can explicitly select the workspace Cargo.toml.

All available crate LICENSE/LICENCE/NOTICE/COPYING texts are copied under `third_party/rust/<name-version>/`. Missing SPDX metadata is an actionable failure before writes, never `unknown`. Crates missing packaged notice text are explicitly marked `notice_text_available=false` and warned; the current sole gap is `simd_helpers 0.1.0` (MIT metadata, no license text in either archive or exact upstream commit). This gap is not represented as legal approval. Four other missing archive notices are supplemented from their `.cargo_vcs_info.json` pinned upstream revisions. Available ppocr license texts are copied from its vendored crate. Pinned actual ONNX Runtime 1.20.0 MIT license + ThirdPartyNotices and PaddleOCR Apache-2.0 text are included offline; `third_party/NOTICE_SOURCES.json` records source revisions and hashes. `licenses` validates those pinned hashes and never downloads notice text at runtime. Cargo may need its normal build-time package fetch first.

Actual validation: 223 packages (152 shipped, 71 build-only); 397 notice files hashed, stable repeated inventory; final manifest contains all 397 matching notice hashes plus inventory (398 total files in empty test bundle). CLI test also verifies proc-macro/build/runtime scope, exclusion of dev/xtask-only, missing metadata failure, stable regeneration, and final manifest coverage. `cargo test -p xtask`: now 16 passing integration tests. Fixed manifest Windows path normalization discovered by the coverage test.

## Validation (2026-07-21)

- `cargo test --manifest-path .../issue63-tools/rust/Cargo.toml -p xtask`: 15 CLI integration tests passed. Added red/green coverage for builder sampling/crop/original lookup, complete benchmark schedule/warmup/repeat metrics, empty/malformed OCR response, killed timeout, malformed protobuf, duplicate/invalid UTF-8 dictionary and forbidden-source preservation.
- Actual `verify --check-onnx` and `onnx-smoke` were run with the controlled local model directory and source-built release engine. Both detector and recognizer loaded/inferred on CPU and matched the frozen public ROI fingerprints. Raw player text and operator-local paths are intentionally not repeated in this shared evidence.
- Actual strict `real-roi`: 18 frames, classification match rate 1.0, all status OK, all boxes nonempty. Local ignored report: `rust/target/issue63-real-roi.jsonl`.
- Actual builder using FFmpeg-generated public-fixture video (not private recordings): one English sample, baseline and manifest produced, crop JPEG decoded successfully.

These are tool parity gates, not final issue #63 acceptance: no full private-corpus performance/release regression or clean-machine GUI/offline acceptance is claimed. Download tests cover forbidden source/path escape/preserved output; successful TLS transfer, interrupted HTTPS bodies, and disk-failure injection are not newly exercised here.
