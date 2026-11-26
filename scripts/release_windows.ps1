[CmdletBinding()]
param([string]$InputDirectory, [string]$LockSha256, [string]$SignInstallerScript)
$ErrorActionPreference = 'Stop'
if ($InputDirectory -or $LockSha256) { throw 'Native UI release does not accept WebView2 runtime inputs.' }
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$runtime = $null
Push-Location -LiteralPath $repoRoot
try {
    if (-not (Get-Command makensis -ErrorAction SilentlyContinue)) { throw 'NSIS 3.11+ is required.' }
    if (-not (Get-Command 7z -ErrorAction SilentlyContinue)) { throw '7-Zip is required for non-executing product audit.' }
    $version = [string](Get-Content -LiteralPath 'ui\package.json' -Raw | ConvertFrom-Json).version
    if ($version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw 'Release requires numeric semantic version.' }
    # Build once, then reuse exactly the same CLI/UI bytes in both installers.
    # build_windows retains pinned models/FFmpeg, license, manifest and smoke gates.
    & powershell -NoProfile -ExecutionPolicy Bypass -File "$PSScriptRoot\build_windows.ps1" -SkipInstall
    if ($LASTEXITCODE -ne 0) { throw 'Release CLI build/assurance failed.' }
    & powershell -NoProfile -ExecutionPolicy Bypass -File "$PSScriptRoot\build_native_ui_windows.ps1" -SkipCliBuild
    if ($LASTEXITCODE -ne 0) { throw 'Release native UI build failed.' }
    $output = Join-Path $repoRoot 'dist\release'
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $variants = @('Native')
    foreach ($variant in $variants) {
        $args = @('-NoProfile','-ExecutionPolicy','Bypass','-File',"$PSScriptRoot\build_installers_windows.ps1",'-Variant',$variant,'-SkipCliBuild','-SkipUiBuild','-OutputDirectory',$output,'-Version',$version)

        if ($SignInstallerScript) { $args += @('-SignInstallerScript',$SignInstallerScript) }
        & powershell @args
        if ($LASTEXITCODE -ne 0) { throw "$variant installer build/signing failed." }
        $name = $variant.ToLowerInvariant()
        $installer = Join-Path $output "pubg-highlight-trim-$name-setup.exe"
        & "$PSScriptRoot\verify_installer_payload.ps1" -InstallerPath $installer -Layout (Join-Path $repoRoot "build\installer-stage-$name")
        Move-Item -LiteralPath $installer -Destination (Join-Path $output "pubg-highlight-trim-$version-windows-x64-$name-setup.exe")
    }
    & "$PSScriptRoot\write_release_checksums.ps1" -Directory $output -Version $version
    Write-Warning 'Native UI installer statically audited; NOT_ACCEPTED. No Inno Setup or Windows VM acceptance is claimed.'
} finally {
    Pop-Location
    if ($runtime -and (Test-Path -LiteralPath $runtime)) { Remove-Item -LiteralPath $runtime -Recurse -Force }
}
