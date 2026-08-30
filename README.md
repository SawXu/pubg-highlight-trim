# pubg-highlight-trim

English | [简体中文](README_zh.md)

Windows-only CLI for trimming PUBG NVIDIA Highlight clips around key OCR events.

The repository also ships a native Windows desktop UI. It uses WPF with no browser runtime, launches the CLI as a separate process, and does not reference the Python source code.

Default behavior detects both your knocks/eliminations and enemies knocking/eliminating you, keeps 5 seconds before and 1 second after the detected event, skips events in the first 2 seconds, and keeps 10 seconds before molotov/fire-bomb eliminations.

## Installation

No Python install required.

1. Download `pubg-highlight-trim-windows-x64.zip` from the [latest release](https://github.com/SawXu/pubg-highlight-trim/releases/latest).
2. Extract the zip to any folder.
3. The bundle includes `pubg-highlight-trim.exe`, bundled `ffmpeg.exe`/`ffprobe.exe`, and the PaddleOCR models needed by OCR detection.

For the graphical app, download `pubg-highlight-trim-ui-windows-x64.zip`, extract it, and run `pubg-highlight-trim-ui.exe`. Keep the bundled `cli` directory beside the UI executable because it contains the complete CLI and OCR runtime.

Run it from the extracted folder:

```powershell
.\pubg-highlight-trim.exe "C:\Users\you\AppData\Local\Temp\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰"
```

## Usage

### Desktop UI

The UI covers folder or single-MP4 selection, detection target and language, clip timing, scan mode, worker count, scan-only mode, merged output, and recursive search. During a run it shows per-file progress, include/skip counts, and the raw CLI log. On completion it can open the output folder or play the merged video.

The UI communicates only through CLI arguments, standard output, and the final `summary.json`. Set `PUBG_HIGHLIGHT_TRIM_CLI` to override the CLI executable during development or compatibility testing.

### Command line

Point the CLI at your PUBG NVIDIA Highlight folder (or one or more mp4 files):

```powershell
pubg-highlight-trim "C:\Users\you\AppData\Local\Temp\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰"
```

You can also pass one mp4 directly:

```powershell
pubg-highlight-trim "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰\PLAYERUNKNOWN'S BATTLEGROUNDS 2026.06.28 - 22.30.13.65.淘汰.DVR.mp4"
```

When multiple mp4 files are specified, they are processed from the earliest recording to the latest, based on the timestamp in each PUBG filename, and their trimmed clips are merged by default:

```powershell
pubg-highlight-trim --files "F:\Highlights\video1.mp4" "F:\Highlights\video2.mp4" "F:\Highlights\video3.mp4" --merge ".\selected_merged.mp4"
```

Common options:

```powershell
pubg-highlight-trim "." -o ".\trimmed" --merge ".\merged.mp4" -y
pubg-highlight-trim "F:\NVIDIA\TEMP\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS" -o ".\trimmed_auto" --merge ".\merged_auto.mp4" -y
pubg-highlight-trim "F:\NVIDIA\TEMP\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS" --game-lang en -o ".\trimmed_en" --merge ".\merged_en.mp4" -y
pubg-highlight-trim "." --scan-only --scan-mode full --coarse-step 2 -o ".\fullscan_2s" -y
pubg-highlight-trim "." --scan-mode fast
pubg-highlight-trim "." --profile

# Parallelism is selected automatically; override only when needed
pubg-highlight-trim "." --scan-only -o ".\parallel_scan" -y
pubg-highlight-trim "." --scan-only --jobs 1  # disable parallel workers
```

Run `pubg-highlight-trim --help` to see all options.

### Command-line options

| Option | Default | Description |
| --- | --- | --- |
| `input` (positional) | `.` | PUBG highlight folder or a single mp4 file |
| `--files FILE [FILE ...]` | none | Process only these mp4 files, ordered by the recording timestamp in each PUBG filename |
| `--target` | `both` | Event to detect: `self-death` (enemies knocking/eliminating you), `own-kill` (you knocking/eliminating others), `both` |
| `--game-lang` | `auto` | Game language profile; `auto` detects from NVIDIA Highlight filenames. Choices: `auto`, `zh-Hans`, `zh-Hant`, `en` |
| `-o`, `--output-dir` | auto | Root output directory; individual trimmed clips are written under `clips\` |
| `--before` | `5.0` | Seconds to keep before an event |
| `--after` | `1.0` | Seconds to keep after an event |
| `--min-event-sec` | `2.0` | Skip events earlier than this many seconds; `0` keeps opening events |
| `--molotov-elim-before` | `10.0` | Seconds to keep before molotov/fire-bomb eliminations; `0` disables |
| `--recursive` | off | Also search subdirectories |
| `--dry-run` / `--scan-only` | off | Detect and write CSV/summary without trimming or merging |
| `--merge [MERGED_MP4]` | folder default | Create a merged mp4; optionally set the output path |
| `--no-merge` | file default | Skip merged mp4 output |
| `-y`, `--overwrite` | off | Overwrite output dir/merged file instead of creating unique names |
| `--verbose` | off | Print startup settings and third-party OCR diagnostics; default output suppresses library noise |
| `--profile` | off | Print per-clip timing breakdown (ffprobe, OCR, frame read, trim) |
| `--jobs` | `auto` | Automatically use 1 or 2 per-video OCR workers based on source count, CPU, and available memory; use `1` to disable or a number to override |
| `--ffmpeg` / `--ffprobe` | auto | Explicit path to ffmpeg.exe / ffprobe.exe |

OCR options (advanced):

| Option | Default | Description |
| --- | --- | --- |
| `--scan-mode` | `auto` | `auto` uses fast scan when candidates exist, else full; `fast`/`full` force a mode. Multi-kill and match-end sources always full scan |
| `--candidate-csv` | auto | Prior CSV whose `EventSec` values are used as scan hints; auto-detects `fullscan_*/candidate_events.csv` if omitted |
| `--no-auto-candidate-csv` | off | Disable automatic candidate_events.csv discovery |
| `--priority-window` | `31:43`, `45:53` | Scan this OCR window first; repeatable, format `start:end` |
| `--scan-start` / `--scan-end` | `0.0` / end | Restrict the scanned time range |
| `--coarse-step` | `4.0` | Seconds between coarse-scan frames |
| `--candidate-lookback` / `--candidate-lookahead` | `8.0` / `0.5` | Time window around candidate hints |
| `--candidate-step` | `4.0` | Seconds between candidate-scan frames |
| `--refine-before` / `--refine-after` | `6.0` / `0.4` | Refine window around a coarse hit |
| `--refine-step` | `0.5` | Seconds between refine-scan frames |
| `--roi` | `0.30,0.66,0.70,0.75` | OCR crop ratios `x1,y1,x2,y2` |
| `--ocr-width` | `768` | Downscale OCR ROI to this width; `0` disables |
| `--no-brightness-gate` | off | Disable the OpenCV bright outlined-text gate before coarse OCR |

### Output behavior

- Single-file input defaults to no merged mp4. Folder and multi-file inputs default to merging the individual clips.
- Use `--merge` without a value to force merging, `--merge ".\merged.mp4"` to choose the merged output path, or `--no-merge` to keep only individual clips.
- The selected output directory is the run's root. Trimmed clips are written under its `clips\` subdirectory; the default merged mp4, concat list, CSV, and summary remain in the root.
- Use `--profile` to print per-clip timings for ffprobe, OCR predict, video frame seek/read, trim encoding, and total clip time. The same timing columns are also written to `检测与裁剪记录.csv`.

## Detection

Detection is OCR-only because own-kill and mixed-event trimming require OCR. Before coarse OCR, OpenCV checks the relative region `x=0.26–0.74, y=0.635–0.725` for bright glyphs, dark outlines, and horizontal text structure. Frames without likely event text skip PaddleOCR. The gate only applies to coarse scanning; event refinement still uses full OCR, and match-end sources automatically bypass the gate because victory overlays can dim the text. All regions use relative coordinates and do not depend on video resolution. Use `--no-brightness-gate` to disable the prefilter or `--roi x1,y1,x2,y2` to override the OCR crop.

Current game-language support is `zh-Hans`, `zh-Hant`, and `en`. The default `--game-lang auto` mode detects the PUBG game language from NVIDIA Highlight filenames, including mixed folders where files have different language labels. Use `--game-lang zh-Hans`, `--game-lang zh-Hant`, or `--game-lang en` only when you want to force one profile.

Detection priority:

1. OCR text for both directions: `你用...击倒/淘汰了...`, `...击倒/淘汰了你`, `你在安全区外倒地了`, `YOU KNOCKED OUT/KILLED ...`, `... KNOCKED/KILLS YOU ...`.
2. If the same source video first shows you being knocked and later shows you being eliminated, only the knock clip is kept.
3. Assist text (`助攻`/`协助`) and delayed finish text (`你终于淘汰了...`) are skipped.
4. Replay-perspective files such as `淘汰画面` / `击倒画面` are not supported.

### Scan modes

Scan mode defaults to `auto`: the CLI looks for the latest `fullscan_*/candidate_events.csv` near the input and uses fast candidate/priority-window scanning when it exists; otherwise it performs a full scan to avoid missing unusual event timings. Use `--scan-mode full` to force exhaustive scanning or `--scan-mode fast` to scan only candidate/priority windows. Multi-kill source files such as `Double kill`, `Multi kill`, `雙殺`, and `多殺`, plus match-end source files such as `End of match`, always use full OCR scanning so later kills are not missed.

## ffmpeg

Release builds bundle `ffmpeg.exe` and `ffprobe.exe` under `vendor/ffmpeg` inside the app bundle.

During source development the resolver checks, in order:

1. `--ffmpeg` / `--ffprobe`
2. bundled `vendor/ffmpeg`
3. `PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR`
4. PATH
5. common Shutter Encoder paths

## Development

### Run tests

```powershell
$env:PYTHONPATH="src"
python -m unittest discover -s tests -v
```

### Build Windows release locally

The release build needs a Windows x64 machine with:

- Python 3.12 x64. The project metadata supports Python 3.10+, but the pinned `paddlepaddle==3.2.2` does not provide a Windows wheel for Python 3.14. Python 3.12 is the verified build version.
- .NET 10 SDK, including the Windows Desktop targeting pack, for the WPF UI.
- The Windows 10/11 SDK `mt.exe` tool on `PATH`. The build uses it to verify the UI executable's `PerMonitorV2` DPI manifest.
- PowerShell and network access for downloading FFmpeg and, on a first build, OCR models.

The Python build dependencies are declared in `pyproject.toml` and installed with the `ocr` and `build` extras:

- `opencv-contrib-python>=4.9`
- `paddleocr==3.7.0`
- `paddlepaddle==3.2.2`
- `pyinstaller>=6.10`

Create and use a Python 3.12 virtual environment before building:

```powershell
py -3.12 -m venv .venv
.\.venv\Scripts\python.exe -m pip install --upgrade pip
.\.venv\Scripts\python.exe -m pip install -e ".[ocr,build]"
```

Build the CLI bundle first, then publish the GUI and package both together:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\download_ffmpeg.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\build_windows.ps1 `
  -Python .\.venv\Scripts\python.exe
powershell -ExecutionPolicy Bypass -File .\scripts\build_ui_windows.ps1 `
  -SkipCliBuild
```

`build_windows.ps1` prefetches the `en` and `zh-Hans` PaddleOCR models into `vendor\paddlex_cache`, embeds FFmpeg and the models into the CLI bundle, and creates `dist\pubg-highlight-trim-windows-x64.zip`. `build_ui_windows.ps1 -SkipCliBuild` reuses that CLI bundle, publishes a self-contained WPF single-file executable, verifies its DPI manifest, and creates `dist\pubg-highlight-trim-ui-windows-x64.zip`. The UI executable is also available at `dist\pubg-highlight-trim-ui\pubg-highlight-trim-ui.exe`; its package keeps the CLI at `cli\pubg-highlight-trim.exe` beside it.

If the CLI bundle already exists, omit the first build command and run only the UI command. The release archives contain all runtime dependencies, so users need neither Python nor .NET installed.

Run the UI tests separately:

```powershell
dotnet test .\ui\PubgHighlightTrim.Ui.Tests\PubgHighlightTrim.Ui.Tests.csproj --configuration Release
```

The UI release is a self-contained native WPF single-file app. Its release archive carries the complete CLI bundle in a side-by-side `cli` directory, so users need neither .NET nor Python installed.

GitHub Actions builds both CLI and UI Windows release archives on tag pushes like `v0.1.0` or manual `workflow_dispatch`.

## License

This project is licensed under `GPL-3.0-or-later`.

Release archives bundle FFmpeg/FFprobe and PaddleOCR runtime assets. See `THIRD_PARTY_NOTICES.md` for third-party license details and source links.
