[CmdletBinding()]
param(
    [switch]$SkipCliBuild,
    [ValidateSet('Release','Debug')][string]$Configuration = 'Release',
    [string]$OutputDirectory
)

# The supported Windows desktop build. It deliberately does not invoke npm,
# Tauri, WebView2, or an installer: the native NSH executable is the only UI
# artifact. CLI packaging remains the existing build_windows.ps1 contract.
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repoRoot
try {
    foreach ($command in @('cargo', 'powershell')) {
        if ($null -eq (Get-Command $command -ErrorAction SilentlyContinue)) {
            throw "Required command '$command' is missing."
        }
    }
    if (-not $SkipCliBuild) {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build_windows.ps1')
        if ($LASTEXITCODE -ne 0) { throw 'CLI build failed' }
    }
    $cli = Join-Path $repoRoot 'dist\pubg-highlight-trim'
    if (-not (Test-Path -LiteralPath (Join-Path $cli 'pubg-highlight-trim.exe') -PathType Leaf)) {
        throw 'CLI bundle is missing. Build it first or omit -SkipCliBuild.'
    }
    $profile = $Configuration.ToLowerInvariant()
    $target = Join-Path $repoRoot "ui\native-ui\target\$profile\pubg-highlight-trim-native-ui.exe"
    $cargoArgs = @('build', '--manifest-path', 'ui/native-ui/Cargo.toml', '--locked')
    if ($Configuration -eq 'Release') { $cargoArgs += '--release' }
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Native UI Cargo build failed' }
    if (-not (Test-Path -LiteralPath $target -PathType Leaf)) { throw "Native UI executable is missing: $target" }

    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repoRoot 'dist\pubg-highlight-trim-native-ui' }
    $layout = [IO.Path]::GetFullPath($OutputDirectory)
    Remove-Item -LiteralPath $layout -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $layout -Force | Out-Null
    Copy-Item -LiteralPath $target -Destination (Join-Path $layout 'pubg-highlight-trim-native-ui.exe')
    Copy-Item -LiteralPath $cli -Destination (Join-Path $layout 'cli') -Recurse -Force
    Copy-Item -LiteralPath README.md, README_zh.md, LICENSE, THIRD_PARTY_NOTICES.md -Destination $layout -Force
    @'
PUBG Highlight Trim - native Windows UI
The native NSH executable and the complete cli directory are one artifact.
This build does not use Tauri, WebView2, npm, or a GUI ZIP.
Run pubg-highlight-trim-native-ui.exe beside cli/; the installed UI uses the administrator execution model and may show UAC.
'@ | Set-Content -LiteralPath (Join-Path $layout 'RUN_UI.txt') -Encoding UTF8
    Write-Host "Built native UI artifact: $layout"
}
finally { Pop-Location }
