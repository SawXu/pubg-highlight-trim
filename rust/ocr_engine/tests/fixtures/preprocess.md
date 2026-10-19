# Preprocessing compatibility fixtures (#63)

`preprocess.json` freezes the existing Python production preprocessing output,
using OpenCV 5.0.0 on Windows x64 with its default optimized CPU paths. Rust
checks exact bytes or SHA-256, **not** a tolerance, for:

- BGR/RGB order; 15-bit BGR→gray and 12-bit-table uint8 HSV conversion;
- 13×7 fixed pixels `byte[i] = (37*i + 13*(i/7)) % 256`, AREA 5×3 and
  LINEAR 19×11 (all bytes are stored);
- integer and noninteger AREA factors, LINEAR enlargement, single-row/column
  borders, and a 65,536-pixel color grid `(b,g,(13*b+71*g)%256)`;
- three existing, already committed ROI PNGs in
  `tests/fixtures/missed_event_ocr/` (source hashes are frozen in JSON),
  downsampled to 512/321 with production OCR's truncated height;
- a 2560×1440 synthetic canvas formed by tiling the first existing ROI, with
  production primary and assist fractional ROI and 768-wide AREA scaling;
- gate results **and reasons** on all three ROIs at widths 512/768/1024 for
  none/light/full, including LINEAR enlargement, plus synthetic flat frames,
  text, noise, and a white bar. These cover every full-gate rejection reason.

`gate_outlined_text.png` is a synthetic black 768×100 canvas with white `H`
characters at x=20..740 step 24, baseline y=55, OpenCV FONT_HERSHEY_SIMPLEX,
scale 0.9, thickness 2. `gate_noise.png` contains fixed synthetic RGB noise.
Neither adds footage or screenshots from private recordings. Test canvases
are composed only in memory; no original video is read or modified.

`tests/preprocess.rs` also has an opt-in real CPU inference test. Run with
`PUBG_OCR_MODEL_DIR` pointing to the PP-OCR model directory and
`--features onnx-test-runtime --test preprocess -- --ignored`; it asserts
stable subjects in all three ROIs, not private detector internals.

The retired implementation and baseline generator remain in Git history / local
migration notes, not as a second runtime implementation or Python test runner.
