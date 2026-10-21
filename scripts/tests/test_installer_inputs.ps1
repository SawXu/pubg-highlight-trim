# Public packaging boundary: invalid Fixed inputs must fail before any build,
# network request, output cleanup or installer invocation.
$ErrorActionPreference = 'Stop'
$entry = Join-Path $PSScriptRoot '..\build_installers_windows.ps1'
$temp = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
function Assert-Rejected([scriptblock]$Action, [string]$Message) {
    try { & $Action } catch {
        if ($_.Exception.Message -notlike "*$Message*") { throw }
        return
    }
    throw "Expected packaging rejection: $Message"
}
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    Assert-Rejected { & $entry -Variant Fixed } 'requires -FixedRuntimeLicenseManifest'
    $license = Join-Path $temp 'license-evidence.json'
    [IO.File]::WriteAllText($license, 'fixture trust input, not license terms')
    $evidence = @{ FixedRuntimeLicenseManifest=$license; FixedRuntimeLicenseManifestSha256=(Get-FileHash $license -Algorithm SHA256).Hash }
    Assert-Rejected { & $entry -Variant Fixed @evidence } 'requires -FixedRuntimePath'
    Assert-Rejected { & $entry -Variant Fixed @evidence -FixedRuntimePath $temp } 'runtime is incomplete'
    foreach ($name in @('msedgewebview2.exe','msedge.dll','icudtl.dat','resources.pak')) {
        Set-Content -LiteralPath (Join-Path $temp $name) -Value 'not a runtime' -Encoding ASCII
    }
    Assert-Rejected { & $entry -Variant Fixed @evidence -FixedRuntimePath $temp } 'requires -FixedRuntimeSha256'
    Assert-Rejected { & $entry -Variant Fixed @evidence -FixedRuntimePath $temp -FixedRuntimeSha256 ('0' * 64) } 'runtime hash mismatch'
    $actualHash = (Get-FileHash (Join-Path $temp 'msedgewebview2.exe') -Algorithm SHA256).Hash
    $evidence.FixedRuntimeLicenseManifestSha256 = '1'*64
    Assert-Rejected { & $entry -Variant Fixed @evidence -FixedRuntimePath $temp -FixedRuntimeSha256 $actualHash -SkipCliBuild -SkipUiBuild } 'license manifest SHA256 mismatch'
    Write-Host 'Installer public input rejection tests passed.'
} finally { Remove-Item -LiteralPath $temp -Recurse -Force }
