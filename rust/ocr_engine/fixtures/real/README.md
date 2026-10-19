# Real ROI migration regression

This directory contains **18** already-public cropped frames, not the original
private videos. `manifest.json` records the audited coverage: 8 simplified
Chinese, 4 traditional Chinese and 6 English frames. No new private media is
needed for this test.

`baseline.jsonl` is retained as historical annotation, **not** a frame-perfect
machine oracle. During #63 verification the following inconsistencies were
confirmed by inspecting the existing images:

- Its previous manifest said 12 frames / 4 per language, but 18 images and 18 rows
  actually existed.
- Frames 10 and 12 show simplified Chinese; their historical labels said
  traditional Chinese.
- Frame 10 contains a bystander's delayed elimination above the player's direct
  elimination, not a delayed player elimination.
- Frames 12 and 18 each display one knock notification; historical text combined
  notifications from more than one sample. Frame 12's annotated elimination is
  not visible in this image.
- Frame 13 contains a self-knock and unrelated assist text; self-knocks are not
  filtered by the own-kill assist rule.
- Frame 8 includes a preceding HUD line. The old parser includes its noise in the
  subject key. This existing defect is tracked separately in #73, not a
  migration fix or a claim that the historical result is human ground truth.

`reference.json` freezes **actual** image OCR and event results from a detached
`a070076526f918b0ef7f1848994d69363d5c1291` source build, using that commit's
compiled Rust engine and Python event parser. It records the engine SHA-256,
image SHA-256, audited game language and fingerprints of the complete OCR/event
fields. Version 2 deliberately does not repeat real player names or raw event
keys in new fixture metadata or benchmark output. Event-key fingerprints,
target/action/method and weapon-dependent Molotov context are compared exactly;
raw subject/text fingerprints and weapon spelling remain diagnostic. This keeps
actual subject identity strict without mistaking harmless surrounding HUD/JPEG
noise for a new event. Exact parser-field parity for identical OCR text is
separately enforced by the frozen event-contract tests.
The original capture helper lives only in local migration notes; no Python
implementation is kept in this fixture directory.

Important: a similarly named old `offline-final/ocr_engine.exe` was found to
predate the reading-order fix. It returned different text order and missed
frames 4 and 11. Its results were rejected as an invalid current-source baseline;
this reference was regenerated using the engine built from the pinned source.

Run the real-model gate after downloading and verifying the pinned models:

```powershell
cargo run --locked --manifest-path rust/Cargo.toml --release -p xtask -- real-roi `
  --executable rust/target/release/ocr_engine.exe --model-dir <model-dir> `
  --fixture rust/ocr_engine/fixtures/real --output <local-report.jsonl>
```

The tool validates frame identity and compares actual production parser results
(target/action/subject/event keys), not target labels copied from annotations.
Every semantic regression or inference/protocol failure exits nonzero. For the
hash-bound reference, every critical event-identity/context field must match exactly.
OCR text differences are fingerprinted diagnostics, never raw player text in
shared tool output; whitespace is normalized before fingerprinting. Per-frame output preserves evidence for
review. The 18-frame reference passed this gate after the metadata audit; this
is complementary to, not a substitute for, complete source-video regression.
