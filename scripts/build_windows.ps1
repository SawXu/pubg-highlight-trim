param(
    [switch]$SkipInstall,
    [switch]$WithPaddleOcr,
    [switch]$WithRustOcr,
    [string]$Python = "python"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot
# -WithRustOcr is retained as a migration alias; Rust/ONNX is now the default.
if ($WithRustOcr) { Write-Warning "-WithRustOcr is deprecated: Rust/ONNX is already the default." }

if (-not (Test-Path "vendor\ffmpeg\ffmpeg.exe") -or -not (Test-Path "vendor\ffmpeg\ffprobe.exe")) {
    & powershell -ExecutionPolicy Bypass -File "scripts\download_ffmpeg.ps1"
    if ($LASTEXITCODE -ne 0) { throw "FFmpeg download failed" }
}

if (-not $SkipInstall) {
    & $Python -m pip install --upgrade pip
    if ($LASTEXITCODE -ne 0) { throw "pip upgrade failed with exit code $LASTEXITCODE" }
    $extras = if ($WithPaddleOcr) { ".[paddle,build]" } else { ".[ocr,build]" }
    & $Python -m pip install -e $extras pyyaml
    if ($LASTEXITCODE -ne 0) { throw "dependency install failed with exit code $LASTEXITCODE" }
}

Remove-Item -LiteralPath "build", "dist" -Recurse -Force -ErrorAction SilentlyContinue
$ffmpegData = "vendor\ffmpeg;vendor\ffmpeg"
$pyArgs = @("-m", "PyInstaller", "--noconfirm", "--clean", "--name", "pubg-highlight-trim", "--console", "--exclude-module", "tkinter", "--exclude-module", "PIL.ImageTk", "--exclude-module", "paddleocr", "--exclude-module", "paddle", "--exclude-module", "paddlex", "--collect-all", "cv2", "--add-data", $ffmpegData, "src\pubg_highlight_trim\__main__.py")
if ($WithPaddleOcr) {
    & $Python scripts\prefetch_ocr_models.py --cache-dir vendor\paddlex_cache
    if ($LASTEXITCODE -ne 0) { throw "Paddle OCR model prefetch failed" }
    $pyArgs = @("-m", "PyInstaller", "--noconfirm", "--clean", "--name", "pubg-highlight-trim", "--console", "--exclude-module", "tkinter", "--exclude-module", "PIL.ImageTk", "--add-data", $ffmpegData, "--collect-all", "paddleocr", "--collect-all", "paddle", "--collect-all", "paddlex", "--collect-all", "cv2", "--collect-all", "pyclipper", "--collect-all", "shapely", "--copy-metadata", "paddleocr", "--copy-metadata", "paddlex", "--copy-metadata", "imagesize", "--copy-metadata", "opencv-contrib-python", "--copy-metadata", "pyclipper", "--copy-metadata", "pypdfium2", "--copy-metadata", "python-bidi", "--copy-metadata", "shapely", "--add-data", "vendor\paddlex_cache;vendor\paddlex_cache", "src\pubg_highlight_trim\__main__.py")
}
& $Python @pyArgs
if ($LASTEXITCODE -ne 0) { throw "PyInstaller failed with exit code $LASTEXITCODE" }
if ($WithPaddleOcr) { & $Python scripts\prune_windows_bundle.py "dist\pubg-highlight-trim"; if ($LASTEXITCODE -ne 0) { throw "Bundle prune failed" } }

# The default package ships the offline Rust engine and PP-OCRv6 ONNX assets.
$oldRustFlags = $env:RUSTFLAGS
try {
    $env:RUSTFLAGS = "$oldRustFlags -C link-args=/DELAYLOAD:directml.dll -C link-args=delayimp.lib".Trim()
    cargo build --locked --manifest-path rust\Cargo.toml --features cli,directml --release
    if ($LASTEXITCODE -ne 0) { throw "Rust OCR engine build failed with exit code $LASTEXITCODE" }
} finally {
    if ($null -eq $oldRustFlags) { Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue }
    else { $env:RUSTFLAGS = $oldRustFlags }
}
Copy-Item "rust\target\release\ocr_engine.exe" "dist\pubg-highlight-trim\ocr_engine.exe" -Force
foreach ($crtDll in @("vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll")) {
    $source = "dist\pubg-highlight-trim\_internal\$crtDll"
    if (-not (Test-Path $source)) { $source = "$env:SystemRoot\System32\$crtDll" }
    if (-not (Test-Path $source)) { throw "Cannot locate $crtDll for the Rust OCR engine" }
    Copy-Item $source "dist\pubg-highlight-trim\$crtDll" -Force
}
New-Item -ItemType Directory -Path "dist\pubg-highlight-trim\ocr_models" -Force | Out-Null
& $Python scripts\download_ocr_models.py --output-dir "dist\pubg-highlight-trim\ocr_models"
if ($LASTEXITCODE -ne 0) { throw "Rust OCR model download failed" }
Copy-Item "rust\ocr_engine\models\pp_ocrv6_medium.json" "dist\pubg-highlight-trim\ocr_models\pp_ocrv6_medium.json" -Force
@"
@echo off
set PUBG_OCR_BACKEND=rust
"%~dp0pubg-highlight-trim.exe" %*
"@ | Set-Content "dist\pubg-highlight-trim\pubg-highlight-trim-rust.cmd" -Encoding ASCII

Copy-Item "README.md", "README_zh.md", "LICENSE", "THIRD_PARTY_NOTICES.md" -Destination "dist\pubg-highlight-trim" -Force
if (-not $WithPaddleOcr) {
    & $Python scripts\audit_ocr_bundle.py "dist\pubg-highlight-trim"
    if ($LASTEXITCODE -ne 0) { throw "OCR bundle audit failed" }
}
& $Python scripts\build_supply_manifest.py "dist\pubg-highlight-trim"
if ($LASTEXITCODE -ne 0) { throw "Supply chain manifest failed" }
& powershell -ExecutionPolicy Bypass -File scripts\smoke_windows_bundle.ps1 -Bundle "dist\pubg-highlight-trim"
if ($LASTEXITCODE -ne 0) { throw "Windows bundle smoke test failed" }
$zipName = if ($WithPaddleOcr) { "pubg-highlight-trim-paddle-windows-x64.zip" } else { "pubg-highlight-trim-windows-x64.zip" }
$zipPath = "dist\$zipName"
Remove-Item $zipPath -Force -ErrorAction SilentlyContinue
Compress-Archive -Path "dist\pubg-highlight-trim\*" -DestinationPath $zipPath -CompressionLevel Optimal -Force
& $Python scripts\report_bundle_size.py "dist\pubg-highlight-trim" --zip $zipPath
if ($LASTEXITCODE -ne 0) { throw "Bundle size report failed" }
Write-Host "Built $zipPath"
