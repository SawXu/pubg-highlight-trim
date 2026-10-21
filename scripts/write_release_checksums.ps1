[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')][string]$Version,
    [string]$RuntimeLockSha256
)
$ErrorActionPreference = 'Stop'
$names = @("pubg-highlight-trim-$Version-windows-x64-system-setup.exe", "pubg-highlight-trim-$Version-windows-x64-fixed-setup.exe", 'pubg-highlight-trim-windows-x64.zip')
$files = @(foreach ($name in $names) {
    $file = Get-Item -LiteralPath (Join-Path $Directory $name) -ErrorAction Stop
    [ordered]@{ name=$name; bytes=$file.Length; sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
})
$files | ForEach-Object { "$($_.sha256)  $($_.name)" } | Set-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS.txt') -Encoding ASCII
$commit = (& git rev-parse HEAD | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Cannot determine release source commit.' }
[ordered]@{
    schema='pubg-highlight-trim/release-evidence/v1'; version=$Version; commit=$commit
    acceptance='NOT_ACCEPTED'; runtime_lock_sha256=$RuntimeLockSha256
    limitations=@('Static extracted payload checks are not installation or runtime acceptance.', 'Windows 10/11 disposable VM lifecycle, offline Fixed runtime and UI/media acceptance remain required.', 'Unsigned unless separately signed and verified; never disable SmartScreen/security software.')
    files=$files
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Directory 'release-evidence.json') -Encoding UTF8
