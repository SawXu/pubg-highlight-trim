param(
    [switch]$SkipInstall,
    [switch]$WithRustOcr,
    [string]$Python = "python"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

if (-not (Test-Path "vendor\ffmpeg\ffmpeg.exe") -or -not (Test-Path "vendor\ffmpeg\ffprobe.exe")) {
    & powershell -ExecutionPolicy Bypass -File "scripts\download_ffmpeg.ps1"
}

if (-not $SkipInstall) {
    & $Python -m pip install --upgrade pip
    if ($LASTEXITCODE -ne 0) { throw "pip upgrade failed with exit code $LASTEXITCODE" }
    & $Python -m pip install -e ".[ocr,build]"
    if ($LASTEXITCODE -ne 0) { throw "dependency install failed with exit code $LASTEXITCODE" }
}

& $Python scripts\prefetch_ocr_models.py --cache-dir vendor\paddlex_cache
if ($LASTEXITCODE -ne 0) { throw "OCR model prefetch failed with exit code $LASTEXITCODE" }

Remove-Item -LiteralPath "build", "dist" -Recurse -Force -ErrorAction SilentlyContinue

$ffmpegData = "vendor\ffmpeg;vendor\ffmpeg"
$modelData = "vendor\paddlex_cache;vendor\paddlex_cache"
& $Python -m PyInstaller `
    --noconfirm `
    --clean `
    --name pubg-highlight-trim `
    --console `
    --exclude-module tkinter `
    --exclude-module PIL.ImageTk `
    --collect-all paddleocr `
    --collect-all paddle `
    --collect-all paddlex `
    --collect-all cv2 `
    --collect-all pyclipper `
    --collect-all shapely `
    --copy-metadata paddleocr `
    --copy-metadata paddlex `
    --copy-metadata imagesize `
    --copy-metadata opencv-contrib-python `
    --copy-metadata pyclipper `
    --copy-metadata pypdfium2 `
    --copy-metadata python-bidi `
    --copy-metadata shapely `
    --add-data $ffmpegData `
    --add-data $modelData `
    "src\pubg_highlight_trim\__main__.py"
if ($LASTEXITCODE -ne 0) { throw "PyInstaller failed with exit code $LASTEXITCODE" }

& $Python scripts\prune_windows_bundle.py "dist\pubg-highlight-trim"
if ($LASTEXITCODE -ne 0) { throw "Bundle prune failed with exit code $LASTEXITCODE" }

if ($WithRustOcr) {
    $env:RUSTFLAGS = "-C link-args=/DELAYLOAD:directml.dll -C link-args=delayimp.lib"
    cargo build --manifest-path rust\Cargo.toml --features cli,directml --release
    Remove-Item Env:\RUSTFLAGS
    if ($LASTEXITCODE -ne 0) { throw "Rust OCR engine build failed with exit code $LASTEXITCODE" }
    Copy-Item -LiteralPath "rust\target\release\ocr_engine.exe" -Destination "dist\pubg-highlight-trim\ocr_engine.exe" -Force
    # ocr_engine.exe lives at the bundle root and cannot see _internal; ship the
    # MSVC runtime DLLs it imports (msvcp140*, vcruntime140*) next to it.
    foreach ($crtDll in @("vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll")) {
        $source = "dist\pubg-highlight-trim\_internal\$crtDll"
        if (-not (Test-Path $source)) { $source = "$env:SystemRoot\System32\$crtDll" }
        if (-not (Test-Path $source)) { throw "Cannot locate $crtDll for the Rust OCR engine" }
        Copy-Item -LiteralPath $source -Destination "dist\pubg-highlight-trim\$crtDll" -Force
    }
    & $Python scripts\download_ocr_models.py --output-dir dist\pubg-highlight-trim\ocr_models
    if ($LASTEXITCODE -ne 0) { throw "Rust OCR model download failed with exit code $LASTEXITCODE" }
    Copy-Item -LiteralPath "rust\ocr_engine\models\pp_ocrv6_medium.json" -Destination "dist\pubg-highlight-trim\ocr_models\pp_ocrv6_medium.json" -Force
    @"
@echo off
set PUBG_OCR_BACKEND=rust
"%~dp0pubg-highlight-trim.exe" %*
"@ | Set-Content -LiteralPath "dist\pubg-highlight-trim\pubg-highlight-trim-rust.cmd" -Encoding ASCII
}

Copy-Item -LiteralPath "README.md" -Destination "dist\pubg-highlight-trim\README.md" -Force
Copy-Item -LiteralPath "LICENSE" -Destination "dist\pubg-highlight-trim\LICENSE" -Force
Copy-Item -LiteralPath "THIRD_PARTY_NOTICES.md" -Destination "dist\pubg-highlight-trim\THIRD_PARTY_NOTICES.md" -Force
& $Python scripts\build_supply_manifest.py "dist\pubg-highlight-trim"
if ($LASTEXITCODE -ne 0) { throw "Supply chain manifest failed with exit code $LASTEXITCODE" }
@"
Run examples:

  .\pubg-highlight-trim.exe "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS"
  .\pubg-highlight-trim.exe "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\PLAYERUNKNOWN'S BATTLEGROUNDS 2026.06.28 - 22.30.13.65.淘汰.DVR.mp4"

Rust OCR variant (when pubg-highlight-trim-rust.cmd is present):

  .\pubg-highlight-trim-rust.cmd "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS"
  set PUBG_OCR_PROVIDER=directml   REM optional GPU acceleration, falls back to CPU

The bundle includes ffmpeg/ffprobe, OCR models and the Rust OCR engine. No Python
install is required and the runtime performs no network downloads.
License: GPL-3.0-or-later. See LICENSE, THIRD_PARTY_NOTICES.md and
SUPPLY_CHAIN_MANIFEST.json for component provenance and hashes.
"@ | Set-Content -LiteralPath "dist\pubg-highlight-trim\RUN_EXAMPLES.txt" -Encoding UTF8
$zipName = if ($WithRustOcr) { "pubg-highlight-trim-rust-windows-x64.zip" } else { "pubg-highlight-trim-windows-x64.zip" }
$zipPath = "dist\$zipName"
Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
Compress-Archive -Path "dist\pubg-highlight-trim\*" -DestinationPath $zipPath -CompressionLevel Optimal -Force
& $Python scripts\report_bundle_size.py "dist\pubg-highlight-trim" --zip $zipPath
if ($LASTEXITCODE -ne 0) { throw "Bundle size report failed with exit code $LASTEXITCODE" }
Write-Host "Built $zipPath"
