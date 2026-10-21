[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$InstallerPath,
    [Parameter(Mandatory)][string]$Layout,
    [string]$SevenZip = '7z'
)
# Static product audit only: extract with 7-Zip, never execute/install the EXE.
$ErrorActionPreference = 'Stop'
$InstallerPath = (Resolve-Path -LiteralPath $InstallerPath).Path
$Layout = (Resolve-Path -LiteralPath $Layout).Path
$temp = Join-Path ([IO.Path]::GetTempPath()) ('pht-payload-audit-'+[guid]::NewGuid().ToString('N'))
try {
    & $SevenZip x '-y' "-o$temp" $InstallerPath | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'Installer extraction failed.' }
    $markers = @(Get-ChildItem -LiteralPath $temp -Recurse -File -Filter distribution.json)
    if ($markers.Count -ne 1) { throw 'Installer must contain exactly one distribution.json.' }
    $payload = $markers[0].Directory.FullName
    $expected = @(Get-ChildItem -LiteralPath $Layout -Recurse -File)
    foreach ($file in $expected) {
        $relative = $file.FullName.Substring($Layout.Length).TrimStart('\','/')
        $actual = Join-Path $payload $relative
        if (-not (Test-Path -LiteralPath $actual -PathType Leaf)) { throw "Installer payload file missing: $relative" }
        if ((Get-FileHash -LiteralPath $actual -Algorithm SHA256).Hash -ine (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash) { throw "Installer payload hash mismatch: $relative" }
    }
    # 7-Zip materializes NSIS WriteUninstaller as a generated PE alongside payload.
    # Only that exact root-relative addition is permitted; all source payload
    # files above still require exact hashes, and any other addition is fatal.
    $uninstaller = Join-Path $payload 'Uninstall.exe'
    $generatedCount = 0
    if (Test-Path -LiteralPath $uninstaller -PathType Leaf) {
        $stream = [IO.File]::OpenRead($uninstaller)
        try { if ($stream.ReadByte() -ne 0x4d -or $stream.ReadByte() -ne 0x5a) { throw 'Generated NSIS uninstaller is not a PE file.' } } finally { $stream.Dispose() }
        $generatedCount = 1
    }
    if (@(Get-ChildItem -LiteralPath $payload -Recurse -File).Count -ne ($expected.Count + $generatedCount)) { throw 'Unexpected files in installer payload.' }
    Write-Host "Static installer payload audit passed ($($expected.Count) files); runtime/lifecycle NOT accepted."
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}
