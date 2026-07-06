# pubg-highlight-trim

Windows-only CLI for trimming PUBG NVIDIA Highlight clips around key OCR events.

Default behavior detects both your knocks/eliminations and enemies knocking/eliminating you, keeps 5 seconds before and 1 second after the detected event, skips events in the first 2 seconds, and keeps 10 seconds before molotov/fire-bomb eliminations.

## Detection priority

Current game-language support is `zh-Hans`, `zh-Hant`, and `en`. The default `--game-lang auto` mode detects the PUBG game language from NVIDIA Highlight filenames, including mixed folders where files have different language labels. The matching rules for PUBG OCR text and NVIDIA Highlight filenames live in language profiles so more game languages can be added later from real samples. Use `--game-lang zh-Hans`, `--game-lang zh-Hant`, or `--game-lang en` only when you want to force one profile.

1. OCR text for both directions: `你用...击倒/淘汰了...`, `...击倒/淘汰了你`, `你在安全区外倒地了`, `YOU KNOCKED OUT/KILLED ...`, `... KNOCKED/KILLS YOU ...`.
2. If the same source video first shows you being knocked and later shows you being eliminated, only the knock clip is kept.
3. Assist text (`助攻`/`协助`) and delayed finish text (`你终于淘汰了...`) are skipped.
4. Replay-perspective files such as `淘汰画面` / `击倒画面` are not supported.

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
pubg-highlight-trim "." -o ".\trimmed" --merge ".\merged.mp4" -y
pubg-highlight-trim "F:\NVIDIA\TEMP\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS" -o ".\trimmed_auto" --merge ".\merged_auto.mp4" -y
pubg-highlight-trim "F:\NVIDIA\TEMP\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS" --game-lang en -o ".\trimmed_en" --merge ".\merged_en.mp4" -y
pubg-highlight-trim "." --scan-only --scan-mode full --coarse-step 2 -o ".\fullscan_2s" -y
pubg-highlight-trim "." --scan-mode fast
pubg-highlight-trim "." --profile
```

Detection is OCR-only because own-kill and mixed-event trimming require OCR.

Scan mode defaults to `auto`: the CLI looks for the latest `fullscan_*/candidate_events.csv` near the input and uses fast candidate/priority-window scanning when it exists; otherwise it performs a full scan to avoid missing unusual event timings. Use `--scan-mode full` to force exhaustive scanning or `--scan-mode fast` to scan only candidate/priority windows. Multi-kill source files such as `Double kill`, `Multi kill`, `雙殺`, and `多殺` always use full OCR scanning so later kills are not missed.

OCR scans the fixed lower-center PUBG event text area by default. Coarse scan stays at one frame every 4 seconds, and a coarse hit is refined at 0.5-second intervals to find the first visible event text. If a layout differs, override the crop with `--roi x1,y1,x2,y2`.

Single-file input defaults to no merged mp4. Folder input defaults to merging the individual clips. Use `--merge` without a value to force merging, `--merge ".\merged.mp4"` to choose the merged output path, or `--no-merge` to keep only individual clips.

Use `--profile` to print per-clip timings for ffprobe, OCR predict time, video frame seek/read time, trim encoding time, and total clip time. The same timing columns are also written to `检测与裁剪记录.csv`.

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

The release zip contains `pubg-highlight-trim.exe`, bundled `ffmpeg/ffprobe`, and the PaddleOCR models needed by OCR detection. No Python install is required for users of the zip.

GitHub Actions builds the Windows release zip on tag pushes like `v0.1.0` or manual `workflow_dispatch`.

## License

This project is licensed under `GPL-3.0-or-later`.

Release archives bundle FFmpeg/FFprobe and PaddleOCR runtime assets. See `THIRD_PARTY_NOTICES.md` for third-party license details and source links.

## Test

```powershell
$env:PYTHONPATH="src"
python -m unittest discover -s tests -v
```

