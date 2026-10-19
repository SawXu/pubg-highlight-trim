# 2026-10-13 missed-event regression

This is historical diagnosis/verification evidence, including the commands and
Python adapter that existed at the time. Current Rust execution for the same
already-public ROI cases is documented in
[`tests/fixtures/missed_event_ocr/README.md`](../tests/fixtures/missed_event_ocr/README.md).
The current app/build does not use the historical Python commands below; no new
private recording verification was performed by the retirement commit.

## Fixes

- Rust OCR groups boxes into text rows before left-to-right ordering. Row tolerance uses the smaller box height and a fixed anchor; a tall box cannot absorb an adjacent short row (covered by a failing-then-passing regression). Previously `(min_y, min_x)` put a right-hand box before the sentence prefix whenever its top edge was a few pixels higher. Text, confidence scores and boxes retain the same order.
- Explicit files outside the selected detection target can still infer their game language. In particular, `self-death` on a traditional-Chinese match-end file no longer falls back to simplified Chinese. Directory selection and ambiguous death-file voting retain their existing behavior.
- Direct own-kill eliminations are suppressed after a matching knock only within the existing event deduplication window, not for the entire recording. Self-death suppression and explicit delayed-elimination/assist filters are unchanged.
- Detection cache version is bumped; Rust reading-order identity is also versioned so pre-fix cached misses cannot survive the upgrade.

## Automated checks

```powershell
$env:PYTHONPATH='src'
python -m pytest -q
cargo test --manifest-path rust/Cargo.toml
cargo build --manifest-path rust/Cargo.toml --release --features cli

# Enable actual inference tests, otherwise these three tests skip.
$env:PUBG_OCR_ENGINE='<new ocr_engine.exe>'
$env:PUBG_OCR_MODEL_DIR='<PP-OCRv6 model directory>'
python -m pytest tests/test_missed_event_ocr.py -q
```

Real ROI fixtures in `tests/fixtures/missed_event_ocr` exercise the actual Rust engine, Python adapter and production event extraction. All three fail against the old engine (two classification failures and one incorrect actor identity) and pass against the rebuilt engine.

## Local source-video verification

The ten original `2026.10.13` recordings were scanned against `trim_202610_b05` in both modes:

1. Full scan, default 4-second coarse step, no candidate discovery.
2. Candidate scan using the b05 CSV and default candidate step.

Both use default ROI/768 width, CPU inference, no persistent cache, and no increased sampling density. Batch verification uses `--min-event-sec 0` to distinguish detection from intentional opening-event filtering. The explicit `.09` self-death case was additionally verified with automatic language and the default 2-second minimum in both modes.

| Recording suffix | Full scan | b05 candidate scan |
| --- | --- | --- |
| `23.04.41.02` | own-kill 30.500 | own-kill 30.500 |
| `23.09.16.04` | own-kill 30.500 / 63.500 / 78.000 | same |
| `23.20.15.09` | self-death 2.000 | self-death 2.000 |
| `23.51.17.11` | existing self-death events retained at 30.500 / 34.500 | same |

The b05 event recorded at 64 seconds is found at 63.5 seconds by refinement, not lost. `.09` explicitly selected with `--target self-death --game-lang auto` resolves to `zh-Hant` and is included at 2.000 seconds with the default minimum.

### The reported `.03@67` is not a positive acceptance case

Dense same-frame inspection shows `您終於擊殺XYL_99`, i.e. an explicit delayed elimination excluded by the documented policy. Old and new engines both return that text at 67 seconds and the current classifier excludes it. It is not restored by weakening the delayed-elimination filter. The earlier reported extra detection was not reproduced with these binaries/configuration.

## Results and artifacts

- Python suite with real OCR enabled: 194 passed, 5 skipped, 17 subtests passed. Remaining skips are unrelated opt-in tests.
- Rust unit tests: 10 passed (including the additional tall-box/adjacent-row review case). Full/candidate source-video scans were rerun against this final rebuilt engine.
- Real missed-event ROI checks: 3 passed.
- `git diff --check`: clean.
- Local full/candidate scan CSVs, logs, screenshots and assertion results: `F:/NVIDIA/TEMP/miss-event-diagnosis/fixed/` (`verified.json`). Recheck all output assertions with `python F:/NVIDIA/TEMP/miss-event-diagnosis/verify_fixed_outputs.py`; the source-video rerun harness is `regress_fixed.py` in that same diagnosis root.
- Newly built engine used for verification: `rust/target/release/ocr_engine-order-v2-final.exe` (local build artifact, not committed).

These are detection-only source-video regressions; no source files were moved and no release/UI bundle was rebuilt. The new engine must be distributed together with the updated Python CLI; replacing only one component does not include every fix.
