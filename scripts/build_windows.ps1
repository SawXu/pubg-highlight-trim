param(
    [switch]$SkipInstall
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

if (-not (Test-Path "vendor\ffmpeg\ffmpeg.exe") -or -not (Test-Path "vendor\ffmpeg\ffprobe.exe")) {
    & powershell -ExecutionPolicy Bypass -File "scripts\download_ffmpeg.ps1"
}

if (-not $SkipInstall) {
    python -m pip install --upgrade pip
    python -m pip install -e ".[ocr,build]"
}

python scripts\prefetch_ocr_models.py --cache-dir vendor\paddlex_cache

Remove-Item -LiteralPath "build", "dist" -Recurse -Force -ErrorAction SilentlyContinue

$ffmpegData = "vendor\ffmpeg;vendor\ffmpeg"
$modelData = "vendor\paddlex_cache;vendor\paddlex_cache"
python -m PyInstaller `
    --noconfirm `
    --clean `
    --name pubg-highlight-trim `
    --console `
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

Copy-Item -LiteralPath "README.md" -Destination "dist\pubg-highlight-trim\README.md" -Force
Copy-Item -LiteralPath "LICENSE" -Destination "dist\pubg-highlight-trim\LICENSE" -Force
Copy-Item -LiteralPath "THIRD_PARTY_NOTICES.md" -Destination "dist\pubg-highlight-trim\THIRD_PARTY_NOTICES.md" -Force
@"
Run examples:

  .\pubg-highlight-trim.exe "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰"
  .\pubg-highlight-trim.exe "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰\PLAYERUNKNOWN'S BATTLEGROUNDS 2026.06.28 - 22.30.13.65.淘汰.DVR.mp4"

The bundle includes ffmpeg/ffprobe and PaddleOCR models. No Python install is required.
License: GPL-3.0-or-later. See LICENSE and THIRD_PARTY_NOTICES.md.
"@ | Set-Content -LiteralPath "dist\pubg-highlight-trim\RUN_EXAMPLES.txt" -Encoding UTF8
$zipPath = "dist\pubg-highlight-trim-windows-x64.zip"
Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
Compress-Archive -Path "dist\pubg-highlight-trim\*" -DestinationPath $zipPath -Force
Write-Host "Built $zipPath"
