# pubg-highlight-trim

Windows-only CLI for trimming PUBG NVIDIA Highlight clips to the player self knock/elimination moment.

Default behavior keeps 4 seconds before and 1 second after the detected event, writes individual clips, and optionally merges them into one montage.

## Detection priority

1. OCR self-event text: `击倒了你`, `淘汰了你`, `你在安全区外倒地了`.
2. Health-bar fallback: own bottom-center red downed bar or fixed health bar disappearing.
3. Clips already downed at the beginning are skipped, because they missed the before-knock context.

## Install for development

```powershell
cd G:\Tools\pubg-highlight-trim
python -m pip install -e .
```

OCR mode needs PaddleOCR dependencies:

```powershell
python -m pip install -e ".[ocr]"
```

## Run

```powershell
pubg-highlight-trim "C:\Users\you\AppData\Local\Temp\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰"
```

You can also pass one mp4 directly:

```powershell
pubg-highlight-trim "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰\PLAYERUNKNOWN'S BATTLEGROUNDS 2026.06.28 - 22.30.13.65.淘汰.DVR.mp4"
```

Useful options:

```powershell
pubg-highlight-trim "." --before 4 --after 1 --output-dir ".\trimmed" --final ".\montage.mp4"
pubg-highlight-trim "." --detector ocr
pubg-highlight-trim "." --detector health
pubg-highlight-trim "." --dry-run
pubg-highlight-trim "." --profile
```

The default detector is `auto`: OCR is tried first and health-bar detection is used when OCR dependencies are missing or no self-event text is found for a clip.

OCR scans the fixed lower-center PUBG self-event text area by default. Coarse scan stays at one frame every 3 seconds, and a coarse hit is refined at 0.5-second intervals to find the first visible self-event text. If a layout differs, override the crop with `--roi x1,y1,x2,y2`.

Use `--profile` to print per-clip timings for ffprobe, opening downed check, OCR predict time, video frame seek/read time, trim encoding time, and total clip time. The same timing columns are also written to `检测与裁剪记录.csv`.

## ffmpeg

Release builds bundle `ffmpeg.exe` and `ffprobe.exe` under `vendor/ffmpeg` inside the app bundle.

During source development the resolver checks, in order:

1. `--ffmpeg` / `--ffprobe`
2. bundled `vendor/ffmpeg`
3. `PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR`
4. PATH
5. common Shutter Encoder paths

## Build Windows release locally

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\download_ffmpeg.ps1
python -m pip install -e ".[ocr,build]"
powershell -ExecutionPolicy Bypass -File .\scripts\build_windows.ps1
```

The release zip contains `pubg-highlight-trim.exe`, bundled `ffmpeg/ffprobe`, and the PaddleOCR models needed by the default OCR-first detector. No Python install is required for users of the zip.

GitHub Actions builds the Windows release zip on tag pushes like `v0.1.0` or manual `workflow_dispatch`.

## License

This project is licensed under `GPL-3.0-or-later`.

Release archives bundle FFmpeg/FFprobe and PaddleOCR runtime assets. See `THIRD_PARTY_NOTICES.md` for third-party license details and source links.

## Test

```powershell
$env:PYTHONPATH="src"
python -m unittest discover -s tests -v
```
