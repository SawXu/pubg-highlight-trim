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
    $release = Join-Path $root 'scripts\release_windows.ps1'
    Reject { & $release -InputDirectory (Join-Path $temp 'legacy-runtime') -LockSha256 ('0'*64) } 'does not accept WebView2 runtime inputs'
    $payload = Join-Path $temp 'payload'; New-Item -ItemType Directory -Path $payload | Out-Null
    [IO.File]::WriteAllText("$payload\distribution.json", '{"schema":"pubg-highlight-trim/distribution/v2","artifact":"native-ui","variant":"native","version":"0.2.0"}')
    [IO.File]::WriteAllText("$payload\app.exe", 'fixture')
    Compress-Archive -Path $payload -DestinationPath "$temp\product.zip"
    & (Join-Path $root 'scripts\verify_installer_payload.ps1') -InstallerPath "$temp\product.zip" -Layout $payload
    [IO.File]::AppendAllText("$payload\app.exe", 'changed')
    Reject { & (Join-Path $root 'scripts\verify_installer_payload.ps1') -InstallerPath "$temp\product.zip" -Layout $payload } 'Installer payload hash mismatch'
    $products = Join-Path $temp 'products'; New-Item -ItemType Directory -Path $products | Out-Null
    $name='pubg-highlight-trim-0.2.0-windows-x64-native-setup.exe'
    [IO.File]::WriteAllText((Join-Path $products $name), 'artifact-fixture')
    & (Join-Path $root 'scripts\write_release_checksums.ps1') -Directory $products -Version '0.2.0'
    $sums = Get-Content "$products\SHA256SUMS.txt"
    if (@($sums).Count -ne 1 -or @($sums | Where-Object { $_ -like "*  $name" }).Count -ne 1) { throw 'Incorrect Native product checksums' }
    $evidence = Get-Content "$products\release-evidence.json" -Raw | ConvertFrom-Json
    if ($evidence.acceptance -cne 'NOT_ACCEPTED' -or $evidence.files.Count -ne 1) { throw 'Misleading product evidence' }
    Write-Host 'Native release entry, payload and final checksum tests passed.'
} finally { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
