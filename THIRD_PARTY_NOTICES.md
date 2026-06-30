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

## Source links

- Project source: this repository.
- FFmpeg upstream: `https://ffmpeg.org/`
- BtbN FFmpeg-Builds: `https://github.com/BtbN/FFmpeg-Builds`
- PaddleOCR: `https://github.com/PaddlePaddle/PaddleOCR`
- PaddlePaddle: `https://github.com/PaddlePaddle/Paddle`
- PaddleX: `https://github.com/PaddlePaddle/PaddleX`
- PyInstaller: `https://github.com/pyinstaller/pyinstaller`

## No third-party binary changes

The release workflow downloads and redistributes the FFmpeg runtime as-is. This project does not patch FFmpeg, PaddleOCR, PaddlePaddle, PaddleX, or PyInstaller.
