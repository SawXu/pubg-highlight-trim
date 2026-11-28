[CmdletBinding()]
param(
    [ValidateSet('Native')][string]$Variant = 'Native',
    [string]$OutputDirectory,
    [switch]$SkipCliBuild,
    [switch]$SkipUiBuild,
    [ValidateSet('Release','Debug')][string]$Configuration = 'Release',
    [string]$InstallerEngine,
    [string]$SignInstallerScript,
    [string]$Version
)

# Build the single native UI artifact consumed by the NSIS/PowerShell installer engine.
# Tauri/WebView2 variants are intentionally not accepted.
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repoRoot
try {
    if (-not $PSBoundParameters.ContainsKey('Version')) {
        $manifest = Get-Content -LiteralPath (Join-Path $repoRoot 'ui\native-ui\Cargo.toml') -Raw
        if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"\s*$') { throw 'Native UI package version is missing.' }
        $Version = $Matches[1]
    }
    # Numeric SemVer only: keep the installer/registry version unambiguous.
    if ($Version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        throw 'Installer Version must be a numeric semantic version (major.minor.patch).'
    }
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repoRoot 'dist\installers' }
    $OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
    # Only the fixed build staging path may be recursively cleaned; never a
    # caller-selected installation/output directory.
    $layout = Join-Path $repoRoot "build\installer-stage-$($Variant.ToLowerInvariant())"

    if (-not $SkipCliBuild) {
        & powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build_windows.ps1')
        if ($LASTEXITCODE -ne 0) { throw 'CLI build failed' }
    }
    $cli = Join-Path $repoRoot 'dist\pubg-highlight-trim'
    if (-not (Test-Path -LiteralPath (Join-Path $cli 'pubg-highlight-trim.exe') -PathType Leaf)) { throw 'CLI output is missing; build_windows.ps1 must produce it.' }

    if (-not $SkipUiBuild) {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build_native_ui_windows.ps1') -SkipCliBuild -Configuration $Configuration
        if ($LASTEXITCODE -ne 0) { throw 'Native UI build failed' }
    }
    $ui = Join-Path $repoRoot 'dist\pubg-highlight-trim-native-ui'
    $uiExe = Join-Path $ui 'pubg-highlight-trim-native-ui.exe'
    if (-not (Test-Path -LiteralPath $uiExe -PathType Leaf)) { throw 'UI output is missing; build_native_ui_windows.ps1 must produce it.' }
    if (Test-Path -LiteralPath $layout) { Remove-Item -LiteralPath $layout -Recurse -Force }
    New-Item -ItemType Directory -Path $layout -Force | Out-Null
    Copy-Item -LiteralPath $uiExe -Destination $layout -Force
    Copy-Item -LiteralPath $cli -Destination (Join-Path $layout 'cli') -Recurse -Force
    foreach ($name in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','msvcp140_1.dll')) {
        $dll = Join-Path $cli $name
        if (-not (Test-Path -LiteralPath $dll -PathType Leaf)) { throw "Required UI CRT DLL is missing: $name" }
        Copy-Item -LiteralPath $dll -Destination $layout -Force
    }
    Copy-Item -LiteralPath README.md, README_zh.md, LICENSE, THIRD_PARTY_NOTICES.md -Destination $layout

    # Licenses are payload, not optional build-host metadata. Collect even when
    # reusing compiled binaries; fail before handing any layout to the engine.
    # Native UI has no WebView2/npm license closure. The CLI build has already
    # generated and hashed its complete Rust/FFmpeg license inventory.
    $distribution = [ordered]@{ schema='pubg-highlight-trim/distribution/v2'; artifact='native-ui'; variant='native'; version=$Version }
    [IO.File]::WriteAllText((Join-Path $layout 'distribution.json'), ($distribution | ConvertTo-Json -Compress), (New-Object Text.UTF8Encoding($false)))

    $owned = @(Get-ChildItem -LiteralPath $layout -Recurse -File | Where-Object { $_.Name -ne 'installer-manifest.json' } | ForEach-Object {
        [ordered]@{ path=$_.FullName.Substring($layout.Length).TrimStart('\','/').Replace('\','/'); owner='application'; action='replace-or-remove' }
    })
    [ordered]@{ schema='pubg-highlight-trim/installer-ownership/v1'; variant=$Variant.ToLowerInvariant(); version=$Version; files=$owned } |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $layout 'installer-manifest.json') -Encoding UTF8

    if (-not $InstallerEngine) {
        $candidate = Join-Path $PSScriptRoot 'installer\build_installer.ps1'
        if (Test-Path -LiteralPath $candidate) { $InstallerEngine = $candidate }
    }
    if (-not $InstallerEngine) { throw 'Installer engine is not available. Expected scripts/installer/build_installer.ps1; no installer was silently skipped.' }
    if (-not (Test-Path -LiteralPath $InstallerEngine -PathType Leaf)) { throw "Installer engine not found: $InstallerEngine" }
    & powershell -NoProfile -ExecutionPolicy Bypass -File $InstallerEngine -Layout $layout -OutputDirectory $OutputDirectory -Variant $Variant
    if ($LASTEXITCODE -ne 0) { throw 'Installer engine failed' }
    # Signing is explicit and owned by the release operator. The hook must fail
    # on signing/verification failure and receives the output directory/variant.
    if ($SignInstallerScript) {
        & powershell -NoProfile -ExecutionPolicy Bypass -File $SignInstallerScript -OutputDirectory $OutputDirectory -Variant $Variant
        if ($LASTEXITCODE -ne 0) { throw 'Installer signing/verification hook failed' }
    }
    Write-Host "Built $Variant installer from $layout"
}
finally { Pop-Location }
