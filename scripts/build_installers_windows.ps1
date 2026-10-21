[CmdletBinding()]
param(
    [ValidateSet('System','Fixed')][string]$Variant = 'System',
    [string]$FixedRuntimePath,
    [string]$FixedRuntimeSha256,
    [string]$OutputDirectory,
    [switch]$SkipCliBuild,
    [switch]$SkipUiBuild,
    [ValidateSet('Release','Debug')][string]$Configuration = 'Release',
    [string]$InstallerEngine,
    [string]$SignInstallerScript,
    [string]$Version
)

# Build a directory consumed by the NSIS/PowerShell installer engine.  This is
# intentionally not a ZIP/archive entry point: installation owns file lifetime.
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repoRoot
try {
    if ($Variant -eq 'Fixed') {
        if ([string]::IsNullOrWhiteSpace($FixedRuntimePath)) { throw 'Fixed variant requires -FixedRuntimePath (a trusted, already-unpacked WebView2 Fixed runtime).' }
        $FixedRuntimePath = (Resolve-Path -LiteralPath $FixedRuntimePath).Path
        foreach ($cleanRoot in @((Join-Path $repoRoot 'build'), (Join-Path $repoRoot 'dist'))) {
            if ($FixedRuntimePath -ieq $cleanRoot -or $FixedRuntimePath.StartsWith("$cleanRoot\", [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Fixed runtime must be outside build/dist cleanup paths.'
            }
        }
        $runtimeItems = @((Get-Item -LiteralPath $FixedRuntimePath)) + @(Get-ChildItem -LiteralPath $FixedRuntimePath -Recurse -Force)
        if (@($runtimeItems | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count -gt 0) { throw 'Fixed runtime must not contain reparse points.' }
        foreach ($name in @('msedgewebview2.exe','msedge.dll','icudtl.dat','resources.pak')) {
            if (-not (Test-Path -LiteralPath (Join-Path $FixedRuntimePath $name) -PathType Leaf)) { throw "Fixed WebView2 runtime is incomplete: missing $name" }
        }
        if ([string]::IsNullOrWhiteSpace($FixedRuntimeSha256)) { throw 'Fixed variant requires -FixedRuntimeSha256; network download is never performed.' }
        $runtimeHash = (Get-FileHash -LiteralPath (Join-Path $FixedRuntimePath 'msedgewebview2.exe') -Algorithm SHA256).Hash
        if ($runtimeHash -ine $FixedRuntimeSha256) { throw "Fixed WebView2 runtime hash mismatch (msedgewebview2.exe): expected $FixedRuntimeSha256, got $runtimeHash" }
    }
    if (-not $PSBoundParameters.ContainsKey('Version')) {
        $package = Get-Content -LiteralPath (Join-Path $repoRoot 'ui\package.json') -Raw | ConvertFrom-Json
        $Version = [string]$package.version
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
        & powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build_ui_windows.ps1') -SkipCliBuild -Configuration $Configuration
        if ($LASTEXITCODE -ne 0) { throw 'UI build failed' }
    }
    $ui = Join-Path $repoRoot 'dist\pubg-highlight-trim-ui'
    $uiExe = Join-Path $ui 'pubg-highlight-trim-ui.exe'
    if (-not (Test-Path -LiteralPath $uiExe -PathType Leaf)) { throw 'UI output is missing; build_ui_windows.ps1 must produce it.' }
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

    if ($Variant -eq 'Fixed') {
        Copy-Item -LiteralPath $FixedRuntimePath -Destination (Join-Path $layout 'WebView2Runtime') -Recurse -Force
    }
    $distribution = [ordered]@{ schema='pubg-highlight-trim/distribution/v1'; variant=$Variant.ToLowerInvariant(); version=$Version; minimum_webview2_major=120 }
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
