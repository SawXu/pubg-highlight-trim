[CmdletBinding()]
param([string]$InputDirectory, [string]$LockSha256, [string]$SignInstallerScript, [switch]$SystemOnly)
$ErrorActionPreference = 'Stop'
if ($SystemOnly) {
    if ($InputDirectory -or $LockSha256) { throw 'SystemOnly release does not accept Fixed runtime inputs.' }
} elseif (-not $InputDirectory -or -not $LockSha256) {
    throw 'Dual installer release requires explicit trusted runtime inputs: -InputDirectory and -LockSha256. Use -SystemOnly to release only the shared-WebView2 installer.'
}
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ($InputDirectory) {
    $InputDirectory = [IO.Path]::GetFullPath($InputDirectory).TrimEnd('\','/')
    foreach ($clean in @((Join-Path $repoRoot 'build'), (Join-Path $repoRoot 'dist'))) {
        if ($InputDirectory -ieq $clean -or $InputDirectory.StartsWith($clean+'\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Runtime inputs must be outside build/dist cleanup paths.' }
    }
}
$runtime = Join-Path ([IO.Path]::GetTempPath()) ('pht-fixed-release-'+[guid]::NewGuid().ToString('N'))
Push-Location -LiteralPath $repoRoot
try {
    if (-not $SystemOnly) {
        # Trust gate runs before any build, cleanup, download or installer execution.
        $verified = & "$PSScriptRoot\verify_release_runtime.ps1" -InputDirectory $InputDirectory -LockSha256 $LockSha256 -Destination $runtime
        $licenseManifest = Join-Path $verified 'fixed-runtime-license.json'
        if (-not (Test-Path -LiteralPath $licenseManifest -PathType Leaf)) { throw 'Release requires reviewed fixed-runtime-license.json inside the locked runtime tree.' }
        $licenseHash = (Get-FileHash -LiteralPath $licenseManifest -Algorithm SHA256).Hash
        $runtimePath = Join-Path $verified 'runtime'
        $runtimeExeHash = (Get-FileHash -LiteralPath (Join-Path $runtimePath 'msedgewebview2.exe') -Algorithm SHA256).Hash
    }
    if (-not (Get-Command makensis -ErrorAction SilentlyContinue)) { throw 'NSIS 3.11+ is required.' }
    if (-not (Get-Command 7z -ErrorAction SilentlyContinue)) { throw '7-Zip is required for non-executing product audit.' }
    $version = [string](Get-Content -LiteralPath 'ui\package.json' -Raw | ConvertFrom-Json).version
    if ($version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw 'Release requires numeric semantic version.' }
    # Build once, then reuse exactly the same CLI/UI bytes in both installers.
    # build_windows retains pinned models/FFmpeg, license, manifest and smoke gates.
    & powershell -NoProfile -ExecutionPolicy Bypass -File "$PSScriptRoot\build_windows.ps1" -SkipInstall
    if ($LASTEXITCODE -ne 0) { throw 'Release CLI build/assurance failed.' }
    & powershell -NoProfile -ExecutionPolicy Bypass -File "$PSScriptRoot\build_ui_windows.ps1" -SkipCliBuild
    if ($LASTEXITCODE -ne 0) { throw 'Release UI build failed.' }
    # License collection runs cargo metadata --locked --offline; warm the
    # registry cache first so a cold cache cannot fail the release build.
    & cargo fetch --locked --manifest-path 'ui\src-tauri\Cargo.toml'
    if ($LASTEXITCODE -ne 0) { throw 'UI cargo fetch failed; offline license collection cannot proceed.' }
    $output = Join-Path $repoRoot 'dist\release'
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $variants = if ($SystemOnly) { @('System') } else { @('System','Fixed') }
    foreach ($variant in $variants) {
        $args = @('-NoProfile','-ExecutionPolicy','Bypass','-File',"$PSScriptRoot\build_installers_windows.ps1",'-Variant',$variant,'-SkipCliBuild','-SkipUiBuild','-OutputDirectory',$output,'-Version',$version)
        if ($variant -eq 'Fixed') { $args += @('-FixedRuntimePath',$runtimePath,'-FixedRuntimeSha256',$runtimeExeHash,'-FixedRuntimeLicenseManifest',$licenseManifest,'-FixedRuntimeLicenseManifestSha256',$licenseHash) }
        if ($SignInstallerScript) { $args += @('-SignInstallerScript',$SignInstallerScript) }
        & powershell @args
        if ($LASTEXITCODE -ne 0) { throw "$variant installer build/signing failed." }
        $name = $variant.ToLowerInvariant()
        $installer = Join-Path $output "pubg-highlight-trim-$name-setup.exe"
        & "$PSScriptRoot\verify_installer_payload.ps1" -InstallerPath $installer -Layout (Join-Path $repoRoot "build\installer-stage-$name")
        Move-Item -LiteralPath $installer -Destination (Join-Path $output "pubg-highlight-trim-$version-windows-x64-$name-setup.exe")
    }
    Copy-Item -LiteralPath 'dist\pubg-highlight-trim-windows-x64.zip' -Destination $output
    if ($SystemOnly) {
        & "$PSScriptRoot\write_release_checksums.ps1" -Directory $output -Version $version -SystemOnly
        Write-Warning 'System installer statically audited; NOT_ACCEPTED. Draft only until real Windows VM acceptance is reviewed.'
    } else {
        & "$PSScriptRoot\write_release_checksums.ps1" -Directory $output -Version $version -RuntimeLockSha256 $LockSha256
        Copy-Item -LiteralPath (Join-Path $InputDirectory 'runtime-lock.json') -Destination $output
        Write-Warning 'Dual installer products statically audited; NOT_ACCEPTED. Draft only until real Windows VM acceptance is reviewed.'
    }
} finally {
    Pop-Location
    if (Test-Path -LiteralPath $runtime) { Remove-Item -LiteralPath $runtime -Recurse -Force }
}
