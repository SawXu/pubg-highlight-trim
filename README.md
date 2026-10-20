# pubg-highlight-trim

English | [简体中文](README_zh.md)

Windows-only CLI for trimming PUBG NVIDIA Highlight clips around key OCR events.

The repository also ships a native Windows desktop UI. It uses WPF with no browser runtime and launches the Rust CLI as a separate process.

Default behavior detects both your knocks/eliminations and enemies knocking/eliminating you, keeps 5 seconds before and 1 second after the detected event, skips events in the first 2 seconds, and keeps 10 seconds before molotov/fire-bomb eliminations.

## Installation

No Python install required.

1. Download `pubg-highlight-trim-windows-x64.zip` from the [latest release](https://github.com/SawXu/pubg-highlight-trim/releases/latest).
2. Extract the zip to any folder.
3. The default bundle includes `pubg-highlight-trim.exe`, bundled `ffmpeg.exe`/`ffprobe.exe`, the Rust-only OCR engine, and PP-OCRv6 ONNX models. It does **not** include PaddleOCR, PaddlePaddle, or Paddle models.

For the graphical app, download `pubg-highlight-trim-ui-windows-x64.zip`, extract it, and run `pubg-highlight-trim-ui.exe`. The GUI archive is a WPF desktop front end; the CLI archive is the command-line executable and runtime by itself. In the GUI archive, keep the bundled `cli` directory beside the UI executable (`pubg-highlight-trim-ui.exe` and `cli\pubg-highlight-trim.exe` at the same level). The UI starts that CLI as a child process, so moving or renaming `cli` makes the UI unavailable. Neither archive requires a separate Python or .NET installation.

Run it from the extracted folder:

```powershell
.\pubg-highlight-trim.exe "C:\Users\you\AppData\Local\Temp\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰"
```

## Usage

### Desktop UI

The GUI is intended for Windows users who prefer not to type CLI arguments. After launch it checks the side-by-side CLI and shows its version/status in the window's top status area. Choose either a PUBG NVIDIA Highlight folder or one or more `.mp4` files (multi-select is passed to the CLI as `--files`), then optionally choose an output directory. Leaving the output field empty uses the CLI's defaults.

The controls map to these CLI settings:

- **Detection target**: All, self knocked/eliminated (`self-death`), or your own knocks/eliminations (`own-kill`). The two specific targets may be selected together to produce separate merged videos.
- **Game language**: Auto, Simplified Chinese, Traditional Chinese, or English. Auto detects the profile from Highlight filenames.
- **Scan mode**: Auto, Fast, or Full. Auto uses candidate hints when available; Full is the exhaustive choice when completeness matters.
- **Clip range**: Seconds before and after the event (defaults 5 and 1).
- **Merged output**: Create one merged MP4 in addition to individual clips. A single-file input defaults to no merge in the CLI; the GUI checkbox explicitly sends `--merge` or `--no-merge`.
- **Recursive scan**: Include MP4 files in subdirectories when a folder is selected.
- **Scan only**: Detect events and write records without creating trimmed or merged videos.
- **Overwrite output**: Reuse/clean the selected output paths instead of choosing unique names.
- **Expert settings**: Ignore events in the first N seconds (default 2), keep extra seconds before molotov/fire-bomb eliminations (default 10), choose OCR worker count (`auto` or a positive integer) and independent Trim concurrency (`auto`, `1`, or `2`), and enable/disable the bright outlined-text prefilter.

Click **Start** to run. The right-hand panel reports scan/trim progress, retained and skipped counts, elapsed time, and the current phase. Expand **Show log** to inspect the raw CLI records; this is the first place to look when a run fails. Click **Stop** to cancel the current task; the GUI terminates the CLI and its child processes. After a successful run, **Open output** opens the output root in Explorer. When multiple merged outputs are produced, choose the desired video from that directory.

The UI communicates only through CLI arguments, standard output, and the final `summary.json`. Set `PUBG_HIGHLIGHT_TRIM_CLI` to override the CLI executable during development or compatibility testing.

### GUI troubleshooting

- **CLI not found / unavailable**: confirm that `cli\pubg-highlight-trim.exe` is present beside `pubg-highlight-trim-ui.exe` and that the archive was extracted completely. Do not copy only the UI executable out of the archive. For development or compatibility checks, set `PUBG_HIGHLIGHT_TRIM_CLI` to a valid CLI path.
- **The run fails immediately**: verify that the input path exists, multi-select contains only MP4 files, the output folder is writable, and expert numeric fields are non-negative (`jobs` must be `auto` or a positive integer). Expand the log and note the CLI exit code.
- **No matching events**: check the selected detection target and game language, try **Auto** or **Full** scan, and confirm the files are original NVIDIA Highlight clips rather than unsupported replay-perspective files. Use **Scan only** to inspect `检测与裁剪记录.csv`/`candidate_events.csv` before creating videos.

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

# OCR parallelism is selected automatically; Trim auto conservatively uses 1
pubg-highlight-trim "." --scan-only -o ".\parallel_scan" -y
pubg-highlight-trim "." --scan-only --jobs 1  # disable parallel OCR workers
pubg-highlight-trim "." --jobs 1 --trim-jobs 2 --profile  # independently opt into two Trim workers
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
| `--split-merge-by-target` | off | With `--target both`, merge self-death and own-kill clips into separate videos; default remains the mixed output |
| `-y`, `--overwrite` | off | Overwrite output dir/merged file instead of creating unique names |
| `--verbose` | off | Print startup settings and third-party OCR diagnostics; default output suppresses library noise |
| `--profile` | off | Print per-source timings and final stage timings; `profile.json` is written even without this flag |
| `--jobs` | `auto` | Automatically use 1 or 2 per-video OCR workers based on source count, CPU, and available memory; use `1` to disable or a number to override |
| `--trim-jobs` | `auto` | Independent Trim concurrency: `auto`, `1`, or `2`; auto conservatively selects 1, capped by planned clip count; dry-run/no-clips uses 0 |
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
| `--adaptive-step` | `0.5` | Seconds between samples after a coarse target hit; keeps event confirmation at 0.5-second precision |
| `--adaptive-window` | `2.0` | Forward window after a coarse target hit to sample adaptively; backward confirmation uses refine |
| `--refine-before` / `--refine-after` | `6.0` / `0.4` | Refine window around a coarse hit |
| `--refine-step` | `0.5` | Seconds between refine-scan frames |
| `--roi` | `0.30,0.66,0.70,0.75` | OCR crop ratios `x1,y1,x2,y2` |
| `--ocr-width` | `768` | Downscale OCR ROI to this width; `0` disables |
| `--ocr-provider` | `auto` | Inference mode: `auto`, `cpu`, `directml`, or `directml-only`; case-insensitive, `dml` aliases `directml` |
| `--ocr-device-id` | `0` | Non-negative inference device index (0–2147483647), used by DirectML |
| `--no-brightness-gate` | off | Disable the Rust bright outlined-text gate before coarse OCR |
| `--brightness-gate-mode` | `light` | Gate strictness before coarse OCR: `light` only skips near-empty dark frames, `full` adds outlined-glyph and horizontal-structure checks, `none` disables filtering |

### Performance, progress, and validation

`--jobs` controls OCR scanning only; `--trim-jobs` controls Trim encoding separately. Two Trim workers can compete for CPU/GPU, memory, and disk bandwidth and are not guaranteed to be faster. At most the effective worker count of clips can be running or waiting for publication. Clips are published in planned order, so filenames, CSV and Merge order remain deterministic even if encoding finishes out of order. Try `--trim-jobs 1` if resources are constrained.

The GUI reports Scan, Dedupe, Trim, and Merge separately, with completed/total work, selected workers, and completed/skipped state. Trim counts only validated, published clips, not encoder starts; dry-run skips Trim/Merge rather than reporting planned clips as encoded. Structured `PROGRESS` records are documented in [the progress protocol](docs/progress-protocol.md).

Every successful run writes `profile.json`: `stages` are wall-clock phases, while `aggregates` and per-task timings are nested work and must not be added to the stage totals. With parallel Trim, summed task time may exceed elapsed Trim time. Use `runtime.trim_jobs_requested`/`trim_workers`, source codecs, cache hit/miss counts, current versus cached historical OCR providers, and encoder capability/fallback metadata when comparing runs. `--profile` additionally prints timing records; it does not enable verbose diagnostics. See [the schema](docs/profile-schema.md) and [fixed-source repeated benchmark procedure](docs/trim-performance-benchmark.md).

Validation is always strict: `runtime.trim_validation` is `ffprobe_duration_strict` (`runtime.validation` remains `strict`). Every generated Trim output is probed for duration before atomic publication; Merge output is also probed. There is no fast/disabled validation mode or validation-bypass CLI switch. Separate `trim_output_probe_seconds_sum` from encode work when investigating cost rather than removing validation.

### Output behavior

- Single-file input defaults to no merged mp4. Folder and multi-file inputs default to merging the individual clips.
- Use `--merge` without a value to force merging, `--merge ".\merged.mp4"` to choose the merged output path, or `--no-merge` to keep only individual clips.
- With `--split-merge-by-target --target both`, each source is scanned once and clips are merged per target. Missing targets produce no empty video. Explicit paths receive `_self-death` and `_own-kill` suffixes; default names use Chinese target names on Chinese Windows UI systems and English names otherwise.
- The selected output directory is the run's root. Trimmed clips are written under its `clips\` subdirectory; the default merged mp4, concat list, CSV, and summary remain in the root.
- Use `--profile` to print per-clip timings for ffprobe, OCR predict, video frame seek/read, trim encoding, and total clip time. The same timing columns are also written to `检测与裁剪记录.csv`.

OCR selection uses **CLI > environment > built-in default** independently for each option. `PUBG_OCR_PROVIDER` and the legacy `PUBG_OCR_DIRECTML_DEVICE_ID` remain supported; legacy provider aliases `default` (CPU) and `dml-only` also remain accepted. Explicit empty or whitespace-only provider values (CLI or environment) are invalid; unset the environment variable to use the default. With no settings, `auto` attempts a real DirectML session (DirectX 12) and falls back to CPU if unavailable. No hardware enumeration is performed. Use `--ocr-provider cpu` to restore the v0.3.6 CPU behavior; integrated GPUs may be slower than CPU. For example:

```powershell
.\pubg-highlight-trim.exe "<input-directory>" --ocr-provider directml --ocr-device-id 0 --verbose
```

`directml` falls back to CPU if DirectML initialization or inference fails; `directml-only` fails instead of silently downgrading. `--verbose` reports the selected mode, requested/actual backend, device index, fallback reason, and cache-hit state, including cache-only and failed runs (`actual=unknown` if initialization failed). The cache separates resolved backend, device index, and fallback policy (strict mode never reuses a permissive fallback cache); identical configurations still reuse cached scans. Invalid provider/device values exit with code 2 before producing a summary. Provider diagnostics in `summary.json` include `provider_mode` (`auto`/`cpu`/`directml`/`directml-only`); `provider_requested` identifies the backend actually attempted (`directml` for auto). Fallback reasons distinguish a missing `directml.dll`, no usable DX12 device, and other initialization failures. Existing summary field names/types and CSV columns are unchanged.

GUI and CLI use the same output layout. `<output>\clips\` contains individual trimmed videos; merged videos and their `.concat.txt` FFmpeg lists are in the output root. In split mode, each target with clips gets its own video. `检测与裁剪记录.csv` records per-source detection status, event time, clip range, and timings; `candidate_events.csv` stores candidate hints for later scans; `profile.json` stores phase timings. `summary.json` records source, retained/skipped counts, output paths, scan mode, language, merge state, and OCR/cache metadata; `merge_outputs` lists every generated target output while legacy single-output fields remain compatible.

## Rust application and release artifacts

This branch builds the complete application in Rust: event parsing, scanning, media processing, planning, caching and reporting call the Rust OCR library directly. `ocr_engine.exe` remains an independent JSONL inference diagnostic, not a Python bridge. No CPython, PyInstaller, Python OpenCV/NumPy or Paddle runtime is needed by the application or the official test/build/release commands.

The archive name remains `pubg-highlight-trim-windows-x64.zip`. Paddle compatibility is no longer supported. Legacy `PUBG_OCR_BACKEND` and `PUBG_OCR_ENGINE` bridge settings do not select a backend. `PUBG_OCR_MODEL_DIR` can select an existing model directory. Historical `paddle-*` detection labels and report fields remain for consumer compatibility, not as a backend dependency.

Model hashes are checked at startup. `SUPPLY_CHAIN_MANIFEST.json` records actual bundled files, component sources, licenses and hashes. Models are downloaded only by the explicit build-time tool; running the app never downloads them.

**Migration status:** this integration branch is not final #63 acceptance or a statement about older release archives. Full private-corpus/performance, clean Python-free Windows and final CLI/GUI archive acceptance remain mandatory. Project-owned Python source, tests and scripts have been removed after Rust replacements were established; see the [capability retirement audit](docs/python-retirement-audit.md). This is not a substitute for the remaining acceptance evidence.

## Detection

Detection is OCR-only because own-kill and mixed-event trimming require OCR. Before coarse OCR, Rust image preprocessing prefilters the relative region `x=0.26–0.74, y=0.635–0.725`; the default `light` mode skips OCR only on essentially dark frames, while `--brightness-gate-mode full` restores the stricter outlined-glyph and horizontal-structure checks (cheaper, but it can reject frames that contain thin single-line event text). The gate only applies to coarse scanning; event refinement still uses full OCR, and match-end sources automatically bypass the gate because victory overlays can dim the text. All regions use relative coordinates and do not depend on video resolution. Use `--no-brightness-gate` to disable the prefilter or `--roi x1,y1,x2,y2` to override the OCR crop.

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

From the repository root, with Rust stable (MSVC x64), FFmpeg/FFprobe and .NET 10 SDK installed:

```powershell
cargo test --locked --manifest-path rust/Cargo.toml --workspace --features highlight-cli
cargo test --locked --manifest-path rust/Cargo.toml --features cli,directml
# Explicit real-model gate: requires a verified local model directory, never private recordings
$env:PUBG_OCR_MODEL_DIR="<model-directory>"
cargo test --locked --manifest-path rust/Cargo.toml --features cli --test end_to_end -- --ignored --nocapture
dotnet test ui/PubgHighlightTrim.Ui.Tests/PubgHighlightTrim.Ui.Tests.csproj --configuration Release
```

The default suite uses language-neutral golden data, public ROI images and generated temporary media. Real-model ignored tests are separate acceptance gates, not coverage silently waived by a green default suite. `highlight_plan` is a canonical fixed-data JSON/JSONL planner diagnostic; it cannot preview video/OCR without models.

### Build Windows release locally

The release build needs a Windows x64 machine with:

- Rust stable x64 MSVC toolchain and Visual Studio C++ build tools.
- .NET 10 SDK, including the Windows Desktop targeting pack, for the WPF UI.
- The Windows 10/11 SDK `mt.exe` tool on `PATH`. The build uses it to verify the UI executable's `PerMonitorV2` DPI manifest.
- PowerShell and network access for downloading FFmpeg and, on a first build, OCR models.

No Python virtual environment or pip installation is used. Model download/hash/dictionary, inference smoke, baseline comparison, manifest, archive audit and size tools are Rust `xtask` commands. Run `cargo run --manifest-path rust/Cargo.toml --bin xtask -- --help` for their interfaces. The retirement audit maps each former tool and its effective checks to the Rust replacement.

Build the CLI bundle first, then publish the GUI and package both together:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\download_ffmpeg.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\build_windows.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\build_ui_windows.ps1 `
  -SkipCliBuild
```

The Rust build bundles PP-OCRv6 ONNX assets (not PaddleOCR runtime models) and creates `dist\pubg-highlight-trim-windows-x64.zip`. Paddle compatibility builds are no longer supported. `build_ui_windows.ps1 -SkipCliBuild` reuses that CLI bundle, publishes a self-contained WPF single-file executable, verifies its DPI manifest, and creates `dist\pubg-highlight-trim-ui-windows-x64.zip`. The UI executable is also available at `dist\pubg-highlight-trim-ui\pubg-highlight-trim-ui.exe`; its package keeps the CLI at `cli\pubg-highlight-trim.exe` beside it.

If the CLI bundle already exists, omit the first build command and run only the UI command. The release archives contain all runtime dependencies, so users need neither Python nor .NET installed.

Run the UI tests separately:

```powershell
dotnet test .\ui\PubgHighlightTrim.Ui.Tests\PubgHighlightTrim.Ui.Tests.csproj --configuration Release
```

The UI release is a self-contained native WPF single-file app. Its release archive carries the complete CLI bundle in a side-by-side `cli` directory, so users need neither .NET nor Python installed.

GitHub Actions builds both CLI and UI Windows release archives on tag pushes like `v0.1.0` or manual `workflow_dispatch`.

## License

This project is licensed under `GPL-3.0-or-later`.

Release archives bundle FFmpeg/FFprobe and Rust OCR engine/model assets, without a Paddle compatibility runtime. See `THIRD_PARTY_NOTICES.md` for third-party license details and source links.

Each Windows release also contains `third_party/rust/INVENTORY.json`, generated by `cargo xtask licenses <bundle>` from the locked Cargo metadata. It records the resolved package name/version/SPDX license/source and whether each dependency is shipped or build-only. The corresponding pinned `LICENSE`, `NOTICE`, `COPYING`, ONNX Runtime, PP-OCR/model, CRT and other notice texts are copied under `third_party/`; `SUPPLY_CHAIN_MANIFEST.json` hashes every inventory and notice file. A missing notice or hash mismatch is a release-gate failure.
