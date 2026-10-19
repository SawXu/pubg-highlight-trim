param(
    [switch]$SkipInstall,
    [switch]$WithRustOcr,
    [string]$ModelDirectory,
    [string]$ReleaseDirectory
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
    & $xtaskSource @Arguments
    if ($LASTEXITCODE -ne 0) { throw "xtask $($Arguments -join ' ') failed with exit code $LASTEXITCODE" }
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

# Reuse inputs must survive cleanup; reject an old bundle as an input rather
# than deleting it and only then discovering that its assets were requested.
foreach ($inputDirectory in @($ModelDirectory, $ReleaseDirectory)) {
    if (-not $inputDirectory) { continue }
    $resolved = (Resolve-Path -LiteralPath $inputDirectory).Path.TrimEnd('\', '/')
    foreach ($cleanDirectory in @('build', 'dist')) {
        $cleanPath = Join-Path $repoRoot $cleanDirectory
        if ($resolved -ieq $cleanPath -or $resolved.StartsWith("$cleanPath\", [StringComparison]::OrdinalIgnoreCase)) {
            throw "Reuse directory must be outside build/dist cleanup: $resolved"
        }
    }
}
Remove-Item -LiteralPath "build", "dist" -Recurse -Force -ErrorAction SilentlyContinue
$bundle = Join-Path $repoRoot "dist\pubg-highlight-trim"
$models = Join-Path $bundle "ocr_models"
$ffmpeg = Join-Path $bundle "_internal\vendor\ffmpeg"
New-Item -ItemType Directory -Path $bundle, $models, $ffmpeg -Force | Out-Null

# Standalone builds still compile all release tools. Explicit reuse is accepted
# only with the current commit/configuration manifest and matching binary hashes.
if ($ReleaseDirectory) {
    $ReleaseDirectory = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
    & "$PSScriptRoot\build_rust_release.ps1" -ReleaseDirectory $ReleaseDirectory -VerifyOnly
} else {
    & "$PSScriptRoot\build_rust_release.ps1"
    $ReleaseDirectory = Join-Path $repoRoot "rust\target\release"
}
$appSource = Join-Path $ReleaseDirectory "pubg_highlight_trim.exe"
$engineSource = Join-Path $ReleaseDirectory "ocr_engine.exe"
$xtaskSource = Join-Path $ReleaseDirectory "xtask.exe"
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

if ($ModelDirectory) {
    $ModelDirectory = (Resolve-Path -LiteralPath $ModelDirectory).Path
    Write-Host "Reusing OCR models from $ModelDirectory; pinned hash/ONNX verification remains mandatory."
    Copy-Item -Path (Join-Path $ModelDirectory "*") -Destination $models -Recurse -Force
    # Use the repository's pinned descriptor, never a cached replacement.
    Copy-Item -LiteralPath "rust\ocr_engine\models\pp_ocrv6_medium.json" -Destination (Join-Path $models "pp_ocrv6_medium.json") -Force
} else {
    Invoke-Xtask @("download", "--output-dir", $models)
}
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
