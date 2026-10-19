# Third-Party Notices

This project is licensed as `GPL-3.0-or-later`. The current build scripts produce a Rust application and Rust OCR diagnostic with no CPython, PyInstaller, Python OpenCV/NumPy or Paddle runtime. This describes the current source/build layout, not older release archives or completion of #63 acceptance. No Paddle compatibility archive is built or published. Each archive's `SUPPLY_CHAIN_MANIFEST.json` lists only components actually present.

## Bundled binary/runtime components

| Component | Version/source used by this project | License | Notes |
|---|---|---|---|
| FFmpeg / FFprobe | Downloaded by `scripts/download_ffmpeg.ps1` from `https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip` | GPL-3.0-or-later | The bundled `ffmpeg.exe -L` reports GPL terms. This project does not modify FFmpeg. Corresponding upstream source and build scripts are available from FFmpeg and BtbN FFmpeg-Builds. |
| ppocr-rs | 0.7.3, vendored under `vendor/ppocr-rs` | Apache-2.0 | Rust PP-OCR ONNX pipeline. The vendored copy adds a session-builder hook used to configure CPU and DirectML execution providers. |
| ort / ort-sys | 2.0.0-rc.9 | MIT/Apache-2.0 | Rust ONNX Runtime bindings used by the application and OCR diagnostic. |
| ONNX Runtime | 1.20.0, statically linked into the Rust application and `ocr_engine.exe` via `ort-sys` (`ortrs_static-v1.20.0-x86_64-pc-windows-msvc.tgz` from `https://parcel.pyke.io`, sha256 `BB02D856F2747E4863C344BE38AF2A645E660E9055C871EE9984907497BE590A`) | MIT | CPU execution provider is the default; DirectML is an optional opt-in provider. No runtime model or dependency download is performed by the CLI. |
| DirectML | Windows inbox component (`directml.dll`, Windows 10 1903+) | Proprietary, Microsoft Windows | Optional GPU execution provider for the Rust application and `ocr_engine.exe`. The DLL is delay-loaded and probed before use; when absent or unloadable the engine stays on CPU. `d3d12.dll`, `dxgi.dll` and `dxcore.dll` are inbox Windows components. |
| Visual C++ runtime | `vcruntime140.dll`, `vcruntime140_1.dll`, `msvcp140.dll`, `msvcp140_1.dll` next to both Rust executables | Microsoft software license terms (redistributable) | Required by the Rust application and OCR diagnostic; copied from the build machine's installed Visual C++ 2015-2022 x64 redistributable. Hashes are recorded in `SUPPLY_CHAIN_MANIFEST.json`. |
| PP-OCRv6 medium ONNX assets | Pinned revisions in `rust/ocr_engine/models/pp_ocrv6_medium.json` | Apache-2.0 | Detector, recognizer, and PaddleOCR character dictionary; hashes are verified before use. |

## Source links

- Project source: this repository.
- FFmpeg upstream: `https://ffmpeg.org/`
- BtbN FFmpeg-Builds: `https://github.com/BtbN/FFmpeg-Builds`
- ppocr-rs: `https://github.com/Devolutions/ppocr-rs`
- ONNX Runtime: `https://github.com/microsoft/onnxruntime`
- PP-OCRv6 model assets: Hugging Face revisions recorded in the model manifest.

## No third-party binary changes

The release workflow downloads and redistributes the FFmpeg runtime as-is. This project does not patch FFmpeg. The vendored `ppocr-rs` adds a session-builder hook for execution-provider configuration; its upstream OCR logic remains otherwise unchanged.

## Supply chain manifest

Every file shipped in the release zip except `SUPPLY_CHAIN_MANIFEST.json` itself is hashed in that manifest, which also records component versions, sources and licenses. The runtime never downloads models, binaries, or OCR caches; model files are re-hashed by the Rust application and `ocr_engine.exe` at load time and a mismatch fails explicitly.
