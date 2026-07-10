param(
    [switch]$SkipInstall,
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

Copy-Item -LiteralPath "README.md" -Destination "dist\pubg-highlight-trim\README.md" -Force
Copy-Item -LiteralPath "LICENSE" -Destination "dist\pubg-highlight-trim\LICENSE" -Force
Copy-Item -LiteralPath "THIRD_PARTY_NOTICES.md" -Destination "dist\pubg-highlight-trim\THIRD_PARTY_NOTICES.md" -Force
@"
Run examples:

  .\pubg-highlight-trim.exe "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS"
  .\pubg-highlight-trim.exe "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\PLAYERUNKNOWN'S BATTLEGROUNDS 2026.06.28 - 22.30.13.65.淘汰.DVR.mp4"

The bundle includes ffmpeg/ffprobe and PaddleOCR models. No Python install is required.
License: GPL-3.0-or-later. See LICENSE and THIRD_PARTY_NOTICES.md.
"@ | Set-Content -LiteralPath "dist\pubg-highlight-trim\RUN_EXAMPLES.txt" -Encoding UTF8
$zipPath = "dist\pubg-highlight-trim-windows-x64.zip"
Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
Compress-Archive -Path "dist\pubg-highlight-trim\*" -DestinationPath $zipPath -CompressionLevel Optimal -Force
& $Python scripts\report_bundle_size.py "dist\pubg-highlight-trim" --zip $zipPath
if ($LASTEXITCODE -ne 0) { throw "Bundle size report failed with exit code $LASTEXITCODE" }
Write-Host "Built $zipPath"
