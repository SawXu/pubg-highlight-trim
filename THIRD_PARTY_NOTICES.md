# Third-Party Notices

This project is licensed as `GPL-3.0-or-later`. Release archives also include third-party runtime components so that users can run `pubg-highlight-trim.exe` without installing Python or FFmpeg separately.

## Bundled binary/runtime components

| Component | Version/source used by this project | License | Notes |
|---|---|---|---|
| FFmpeg / FFprobe | Downloaded by `scripts/download_ffmpeg.ps1` from `https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip` | GPL-3.0-or-later | The bundled `ffmpeg.exe -L` reports GPL terms. This project does not modify FFmpeg. Corresponding upstream source and build scripts are available from FFmpeg and BtbN FFmpeg-Builds. |
| PyInstaller bootloader | PyInstaller 6.21.0 | GPL-2.0-or-later with PyInstaller bootloader exception | Used to create `pubg-highlight-trim.exe`. |
| PaddleOCR | 3.7.0 | Apache-2.0 | OCR runtime used to detect self-event text such as `击倒了你` and `淘汰了你`. |
| PaddlePaddle | 3.2.2 | Apache-2.0 | PaddleOCR inference runtime. |
| PaddleX | 3.7.2 | Apache-2.0 | PaddleOCR pipeline dependency. |
| OpenCV contrib Python | 4.10.0.84 or compatible package selected during build | Apache-2.0 | Video frame access and OCR crop preprocessing. |
| pyclipper | 1.4.0 | MIT | PaddleOCR/PaddleX dependency. |
| Shapely | 2.1.2 | BSD-3-Clause | PaddleOCR/PaddleX dependency. |
| pypdfium2 | 5.11.0 | BSD-3-Clause, Apache-2.0, dependency licenses | PaddleOCR/PaddleX dependency. |
| python-bidi | 0.6.10 | LGPL | PaddleOCR/PaddleX dependency. |
| ppocr-rs | 0.7.3, vendored under `vendor/ppocr-rs` | Apache-2.0 | Rust PP-OCR ONNX pipeline. The vendored Cargo manifest is patched to enable the CPU ORT provider only. |
| ort / ort-sys | 2.0.0-rc.9 | MIT/Apache-2.0 | Rust ONNX Runtime bindings used by the optional OCR engine. |
| ONNX Runtime | Loaded by `ort` at build/test time | MIT | CPU execution provider for the Rust OCR engine; no runtime model or dependency download is performed by the CLI. |
| PP-OCRv6 medium ONNX assets | Pinned revisions in `rust/ocr_engine/models/pp_ocrv6_medium.json` | Apache-2.0 | Detector, recognizer, and PaddleOCR character dictionary; hashes are verified before use. |

## Source links

- Project source: this repository.
- FFmpeg upstream: `https://ffmpeg.org/`
- BtbN FFmpeg-Builds: `https://github.com/BtbN/FFmpeg-Builds`
- PaddleOCR: `https://github.com/PaddlePaddle/PaddleOCR`
- PaddlePaddle: `https://github.com/PaddlePaddle/Paddle`
- PaddleX: `https://github.com/PaddlePaddle/PaddleX`
- PyInstaller: `https://github.com/pyinstaller/pyinstaller`
- ppocr-rs: `https://github.com/Devolutions/ppocr-rs`
- ONNX Runtime: `https://github.com/microsoft/onnxruntime`
- PP-OCRv6 model assets: Hugging Face revisions recorded in the model manifest.

## No third-party binary changes

The release workflow downloads and redistributes the FFmpeg runtime as-is. This project does not patch FFmpeg, PaddleOCR, PaddlePaddle, PaddleX, or PyInstaller. The vendored `ppocr-rs` Cargo feature selection is limited to CPU ONNX Runtime support; its upstream source remains otherwise unchanged.
