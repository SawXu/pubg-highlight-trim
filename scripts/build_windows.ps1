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

Remove-Item -LiteralPath "build", "dist" -Recurse -Force -ErrorAction SilentlyContinue

$addData = "vendor\ffmpeg;vendor\ffmpeg"
python -m PyInstaller `
    --noconfirm `
    --clean `
    --name pubg-highlight-trim `
    --console `
    --collect-all paddleocr `
    --collect-all paddle `
    --collect-all paddlex `
    --collect-all cv2 `
    --add-data $addData `
    "src\pubg_highlight_trim\__main__.py"

Copy-Item -LiteralPath "README.md" -Destination "dist\pubg-highlight-trim\README.md" -Force
$zipPath = "dist\pubg-highlight-trim-windows-x64.zip"
Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
Compress-Archive -Path "dist\pubg-highlight-trim\*" -DestinationPath $zipPath -Force
Write-Host "Built $zipPath"
