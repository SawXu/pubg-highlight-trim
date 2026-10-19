param(
    [switch]$SkipInstall,
    [switch]$WithRustOcr
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location -LiteralPath $repoRoot

if ($SkipInstall) { Write-Warning "-SkipInstall is retained for compatibility; this build has no package-install step." }
if ($WithRustOcr) { Write-Warning "-WithRustOcr is deprecated: the Rust application is always built." }

function Require-Command([string]$Name) {
    if ($null -eq (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' was not found. This build intentionally does not install or invoke Python."
    }
}
function Invoke-Xtask([string[]]$Arguments) {
    $manifest = Join-Path $repoRoot "rust\Cargo.toml"
    $workspace = Get-Content -LiteralPath $manifest -Raw -ErrorAction Stop
    if ($workspace -notmatch 'xtask') {
        throw "Rust xtask is not integrated in rust/Cargo.toml yet (issue #68 dependency); release assurance remains blocked."
    }
    & cargo run --locked --manifest-path $manifest --release --bin xtask -- @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo xtask $($Arguments -join ' ') failed with exit code $LASTEXITCODE" }
}
function Assert-NoForbiddenRuntime([string]$Root) {
    $forbidden = @('python', 'python3', 'python312', 'python311', 'pyinstaller', 'paddle', 'paddlex', 'numpy', 'opencv')
    $hits = @()
    foreach ($item in Get-ChildItem -LiteralPath $Root -Recurse -Force -File) {
        $relative = $item.FullName.Substring($Root.Length).TrimStart('\','/')
        foreach ($token in $forbidden) {
            if ($relative.ToLowerInvariant().Contains($token)) { $hits += $relative; break }
        }
    }
    if ($hits.Count -gt 0) { throw "Forbidden Python/Paddle runtime files found: $($hits -join ', ')" }
}

Require-Command cargo
Require-Command powershell
Require-Command Compress-Archive

if (-not (Test-Path -LiteralPath "vendor\ffmpeg\ffmpeg.exe") -or -not (Test-Path -LiteralPath "vendor\ffmpeg\ffprobe.exe")) {
    & powershell -ExecutionPolicy Bypass -File "scripts\download_ffmpeg.ps1"
    if ($LASTEXITCODE -ne 0) { throw "FFmpeg download failed" }
}

Remove-Item -LiteralPath "build", "dist" -Recurse -Force -ErrorAction SilentlyContinue
$bundle = Join-Path $repoRoot "dist\pubg-highlight-trim"
$models = Join-Path $bundle "ocr_models"
$ffmpeg = Join-Path $bundle "_internal\vendor\ffmpeg"
New-Item -ItemType Directory -Path $bundle, $models, $ffmpeg -Force | Out-Null

# The source target keeps its historical Cargo name; the shipped application has
# the public, hyphenated name used by the CLI and WPF contract. Keep DirectML
# optional at runtime by delay-loading directml.dll; CPU does not require it.
$oldRustFlags = $env:RUSTFLAGS
try {
    $env:RUSTFLAGS = "$oldRustFlags -C link-args=/DELAYLOAD:directml.dll -C link-args=delayimp.lib".Trim()
    & cargo build --locked --manifest-path rust\Cargo.toml --release --features cli,directml --bin pubg_highlight_trim
    if ($LASTEXITCODE -ne 0) { throw "Rust application build failed with exit code $LASTEXITCODE" }
    & cargo build --locked --manifest-path rust\Cargo.toml --release --features cli,directml --bin ocr_engine
    if ($LASTEXITCODE -ne 0) { throw "Rust OCR engine build failed with exit code $LASTEXITCODE" }
} finally {
    if ($null -eq $oldRustFlags) { Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue }
    else { $env:RUSTFLAGS = $oldRustFlags }
}
$appSource = "rust\target\release\pubg_highlight_trim.exe"
$engineSource = "rust\target\release\ocr_engine.exe"
if (-not (Test-Path -LiteralPath $appSource -PathType Leaf)) {
    throw "Rust application binary is missing: $appSource (issue #69 is not integrated)"
}
Copy-Item -LiteralPath $appSource -Destination (Join-Path $bundle "pubg-highlight-trim.exe") -Force
if (-not (Test-Path -LiteralPath $engineSource -PathType Leaf)) {
    throw "OCR engine binary is missing: $engineSource (issue #68 integration is incomplete)"
}
Copy-Item -LiteralPath $engineSource -Destination (Join-Path $bundle "ocr_engine.exe") -Force
foreach ($crtDll in @("vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll")) {
    $crtSource = Join-Path $env:SystemRoot "System32\$crtDll"
    if (-not (Test-Path -LiteralPath $crtSource -PathType Leaf)) { throw "Cannot locate required Rust runtime DLL: $crtDll" }
    Copy-Item -LiteralPath $crtSource -Destination (Join-Path $bundle $crtDll) -Force
}

Copy-Item -Path "vendor\ffmpeg\*" -Destination $ffmpeg -Recurse -Force
Copy-Item -LiteralPath "README.md", "README_zh.md", "LICENSE", "THIRD_PARTY_NOTICES.md" -Destination $bundle -Force
Copy-Item -LiteralPath "rust\ocr_engine\models\pp_ocrv6_medium.json" -Destination (Join-Path $models "pp_ocrv6_medium.json") -Force

# #68 supplies these commands. Do not substitute a weaker check while it is absent.
Invoke-Xtask @("download", "--output-dir", $models)
Invoke-Xtask @("verify", "--model-dir", $models, "--check-onnx", "--engine", $engineSource)

@"
@echo off
set "PATH=%~dp0_internal\vendor\ffmpeg;%PATH%"
"%~dp0pubg-highlight-trim.exe" %*
"@ | Set-Content -LiteralPath (Join-Path $bundle "pubg-highlight-trim.cmd") -Encoding ASCII

Invoke-Xtask @("audit", $bundle)
Assert-NoForbiddenRuntime $bundle
# License inventory must be generated before the final manifest so its hash
# coverage is part of the signed supply-chain artifact; failures are fatal.
Invoke-Xtask @("licenses", $bundle)
Invoke-Xtask @("manifest", $bundle)
& powershell -ExecutionPolicy Bypass -File "scripts\smoke_windows_bundle.ps1" -Bundle $bundle
if ($LASTEXITCODE -ne 0) { throw "Windows bundle smoke test failed" }

$zipName = "pubg-highlight-trim-windows-x64.zip"
$zipPath = Join-Path $repoRoot "dist\$zipName"
Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $bundle "*") -DestinationPath $zipPath -CompressionLevel Optimal -Force
Invoke-Xtask @("size", $bundle, "--zip", $zipPath)
Write-Host "Built $zipPath"
