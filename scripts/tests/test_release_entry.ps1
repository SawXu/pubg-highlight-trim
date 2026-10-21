param([string]$MakeNsis)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$temp = Join-Path ([IO.Path]::GetTempPath()) ('pht-release-test-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
function Reject([scriptblock]$Action, [string]$Message) {
    try { & $Action } catch { if ($_.Exception.Message -notlike "*$Message*") { throw }; return }
    throw "Expected rejection: $Message"
}
try {
    $inputDir = Join-Path $temp 'input'
    $source = Join-Path $temp 'source'
    New-Item -ItemType Directory -Path $inputDir,$source,"$source\runtime" | Out-Null
    foreach ($name in @('msedgewebview2.exe','msedge.dll','icudtl.dat','resources.pak')) { [IO.File]::WriteAllText((Join-Path "$source\runtime" $name), 'fixture-not-a-runtime') }
    Compress-Archive -Path "$source\*" -DestinationPath "$inputDir\runtime.zip"
    $files = @(Get-ChildItem $source -Recurse -File | ForEach-Object { @{ path=('runtime/'+$_.Name); sha256=(Get-FileHash $_.FullName -Algorithm SHA256).Hash } })
    @{ schema='pubg-highlight-trim/fixed-runtime/v1'; archive_sha256=(Get-FileHash "$inputDir\runtime.zip" -Algorithm SHA256).Hash; files=$files } | ConvertTo-Json -Depth 5 | Set-Content "$inputDir\runtime-lock.json" -Encoding UTF8
    $lockHash = (Get-FileHash "$inputDir\runtime-lock.json" -Algorithm SHA256).Hash
    $verify = Join-Path $root 'scripts\verify_release_runtime.ps1'
    Reject { & $verify -InputDirectory $inputDir -LockSha256 ('0'*64) -Destination "$temp\runtime" } 'lock SHA256 mismatch'
    if (Test-Path "$temp\runtime") { throw 'Rejected input created runtime output' }
    $runtime = & $verify -InputDirectory $inputDir -LockSha256 $lockHash -Destination "$temp\runtime"
    if (-not (Test-Path "$runtime\runtime\resources.pak" -PathType Leaf)) { throw 'Verified runtime was not returned' }
    [IO.File]::WriteAllText((Join-Path "$source\runtime" 'msedge.dll'), 'tampered')
    Remove-Item "$inputDir\runtime.zip"
    Compress-Archive -Path "$source\*" -DestinationPath "$inputDir\runtime.zip"
    Reject { & $verify -InputDirectory $inputDir -LockSha256 $lockHash -Destination "$temp\bad" } 'archive SHA256 mismatch'
    $lock = Get-Content "$inputDir\runtime-lock.json" -Raw | ConvertFrom-Json
    $lock.archive_sha256 = (Get-FileHash "$inputDir\runtime.zip" -Algorithm SHA256).Hash
    $lock | ConvertTo-Json -Depth 5 | Set-Content "$inputDir\runtime-lock.json" -Encoding UTF8
    $lockHash = (Get-FileHash "$inputDir\runtime-lock.json" -Algorithm SHA256).Hash
    Reject { & $verify -InputDirectory $inputDir -LockSha256 $lockHash -Destination "$temp\bad-tree" } 'file hash mismatch'
    if (Test-Path "$temp\bad-tree") { throw 'Invalid tree retained output' }
    # Archive SHA256 alone must not permit unlisted payloads.
    [IO.File]::WriteAllText((Join-Path $source 'unlisted.dll'), 'unexpected')
    Remove-Item "$inputDir\runtime.zip"
    Compress-Archive -Path "$source\*" -DestinationPath "$inputDir\runtime.zip"
    $lock.archive_sha256 = (Get-FileHash "$inputDir\runtime.zip" -Algorithm SHA256).Hash
    $lock | ConvertTo-Json -Depth 5 | Set-Content "$inputDir\runtime-lock.json" -Encoding UTF8
    $lockHash = (Get-FileHash "$inputDir\runtime-lock.json" -Algorithm SHA256).Hash
    Reject { & $verify -InputDirectory $inputDir -LockSha256 $lockHash -Destination "$temp\unlisted" } 'file set does not match lock'
    if (Test-Path "$temp\unlisted") { throw 'Unlisted archive created output' }
    # Public payload audit uses a ZIP fixture for the external 7-Zip boundary;
    # this does not claim that a fixture is a working NSIS installer.
    $payload = Join-Path $temp 'payload'
    New-Item -ItemType Directory -Path $payload | Out-Null
    [IO.File]::WriteAllText("$payload\distribution.json", '{"variant":"system"}')
    [IO.File]::WriteAllText("$payload\app.exe", 'fixture')
    Compress-Archive -Path $payload -DestinationPath "$temp\product.zip"
    & (Join-Path $root 'scripts\verify_installer_payload.ps1') -InstallerPath "$temp\product.zip" -Layout $payload
    [IO.File]::AppendAllText("$payload\app.exe", 'changed')
    Reject { & (Join-Path $root 'scripts\verify_installer_payload.ps1') -InstallerPath "$temp\product.zip" -Layout $payload } 'Installer payload hash mismatch'
    if ($MakeNsis) {
        # Real NSIS product compilation and static extraction, never execution.
        [IO.File]::WriteAllText("$payload\distribution.json", '{"schema":"pubg-highlight-trim/distribution/v1","variant":"system"}')
        [IO.File]::WriteAllText("$payload\installer-manifest.json", '{"schema":"pubg-highlight-trim/installer-ownership/v1","variant":"system","files":[]}')
        foreach ($variant in @('System','Fixed')) {
            $lower = $variant.ToLowerInvariant()
            [IO.File]::WriteAllText("$payload\distribution.json", ('{"schema":"pubg-highlight-trim/distribution/v1","variant":"'+$lower+'"}'))
            [IO.File]::WriteAllText("$payload\installer-manifest.json", ('{"schema":"pubg-highlight-trim/installer-ownership/v1","variant":"'+$lower+'","files":[]}'))
            & (Join-Path $root 'scripts\installer\build_installer.ps1') -Layout $payload -OutputDirectory "$temp\nsis" -Variant $variant -MakeNsis $MakeNsis
            & (Join-Path $root 'scripts\verify_installer_payload.ps1') -InstallerPath "$temp\nsis\pubg-highlight-trim-$lower-setup.exe" -Layout $payload
        }
    }
    $release = Join-Path $root 'scripts\release_windows.ps1'
    Reject { & $release } 'requires explicit trusted runtime inputs'
    Reject { & $release -InputDirectory (Join-Path $root 'dist\fixed-runtime-input') -LockSha256 $lockHash } 'outside build/dist cleanup'
    # Valid archive without reviewed license metadata must fail before builds.
    Remove-Item (Join-Path $source 'unlisted.dll')
    Remove-Item "$inputDir\runtime.zip"
    Compress-Archive -Path "$source\*" -DestinationPath "$inputDir\runtime.zip"
    $lock.files = @(Get-ChildItem $source -Recurse -File | ForEach-Object { @{ path=('runtime/'+$_.Name); sha256=(Get-FileHash $_.FullName -Algorithm SHA256).Hash } })
    $lock.archive_sha256 = (Get-FileHash "$inputDir\runtime.zip" -Algorithm SHA256).Hash
    $lock | ConvertTo-Json -Depth 5 | Set-Content "$inputDir\runtime-lock.json" -Encoding UTF8
    $lockHash = (Get-FileHash "$inputDir\runtime-lock.json" -Algorithm SHA256).Hash
    Reject { & $release -InputDirectory $inputDir -LockSha256 $lockHash } 'requires reviewed fixed-runtime-license.json'
    $products = Join-Path $temp 'products'
    New-Item -ItemType Directory -Path $products | Out-Null
    foreach ($name in @('pubg-highlight-trim-0.2.0-windows-x64-system-setup.exe','pubg-highlight-trim-0.2.0-windows-x64-fixed-setup.exe','pubg-highlight-trim-windows-x64.zip')) {
        [IO.File]::WriteAllText((Join-Path $products $name), 'artifact-fixture')
    }
    & (Join-Path $root 'scripts\write_release_checksums.ps1') -Directory $products -Version '0.2.0'
    $sums = Get-Content "$products\SHA256SUMS.txt"
    if ($sums.Count -ne 3 -or $sums[0] -cne 'fef393cca2055f043b228a2ee7719db4ca9199a9d9bc28f7affadc684011db94  pubg-highlight-trim-0.2.0-windows-x64-system-setup.exe') { throw 'Incorrect final product checksums' }
    $evidence = Get-Content "$products\release-evidence.json" -Raw | ConvertFrom-Json
    if ($evidence.acceptance -cne 'NOT_ACCEPTED' -or $evidence.files.Count -ne 3) { throw 'Misleading product evidence' }
    Write-Host 'Release entry fail-closed, runtime, product payload and final checksum tests passed.'
} finally { Remove-Item -LiteralPath $temp -Recurse -Force }
