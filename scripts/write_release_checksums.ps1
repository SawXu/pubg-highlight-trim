[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][ValidatePattern('^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')][string]$Version
)
$ErrorActionPreference = 'Stop'
$name = "pubg-highlight-trim-$Version-windows-x64-native-setup.exe"
$file = Get-Item -LiteralPath (Join-Path $Directory $name) -ErrorAction Stop
$record = [ordered]@{ name=$name; bytes=$file.Length; sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
"$($record.sha256)  $name" | Set-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS.txt') -Encoding ASCII
$commit = (& git rev-parse HEAD | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Cannot determine release source commit.' }
[ordered]@{
    schema='pubg-highlight-trim/release-evidence/v1'; version=$Version; commit=$commit
    scope='native'; acceptance='NOT_ACCEPTED'
    limitations=@('Static extracted payload checks are not installation or runtime acceptance.', 'Windows 10/11 disposable VM lifecycle and UI/media acceptance remain required.', 'Unsigned unless separately signed and verified; never disable SmartScreen/security software.')
    files=@($record)
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Directory 'release-evidence.json') -Encoding UTF8
