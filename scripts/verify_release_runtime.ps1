[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$InputDirectory,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$LockSha256,
    [Parameter(Mandatory)][string]$Destination
)
$ErrorActionPreference = 'Stop'
$lockPath = Join-Path $InputDirectory 'runtime-lock.json'
if ((Get-FileHash -LiteralPath $lockPath -Algorithm SHA256).Hash -ine $LockSha256) { throw 'Fixed runtime lock SHA256 mismatch.' }
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
if ($lock.schema -cne 'pubg-highlight-trim/fixed-runtime/v1' -or $lock.archive_sha256 -notmatch '^[a-fA-F0-9]{64}$' -or @($lock.files).Count -eq 0) { throw 'Invalid Fixed runtime lock schema.' }
$archive = Join-Path $InputDirectory 'runtime.zip'
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ine $lock.archive_sha256) { throw 'Fixed runtime archive SHA256 mismatch.' }
if (Test-Path -LiteralPath $Destination) { throw 'Runtime destination must not exist.' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$expected = @{}
foreach ($file in $lock.files) {
    $path = [string]$file.path
    # Windows-normalized, relative file paths only; reject ADS, traversal and aliases.
    if ($path -cnotmatch '^[A-Za-z0-9_-][A-Za-z0-9_./ -]*$' -or $path -match '(^|/)(\.|\.\.)(/|$)|[. ](/|$)' -or $expected.ContainsKey($path) -or $file.sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'Unsafe or duplicate runtime manifest path/hash.' }
    $expected[$path] = [string]$file.sha256
}
$zip = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $archive))
try {
    $seen = @{}
    foreach ($entry in $zip.Entries) {
        $name = $entry.FullName.Replace('\','/')
        if ($name.EndsWith('/')) {
            if ($name -notmatch '^[A-Za-z0-9_-][A-Za-z0-9_./ -]*/$' -or $name -match '(^|/)(\.|\.\.)(/|$)|[. ]/') { throw 'Unsafe runtime archive directory.' }
            continue
        }
        if (-not $expected.ContainsKey($name) -or $seen.ContainsKey($name)) { throw 'Runtime archive file set does not match lock.' }
        $seen[$name] = $true
    }
    if ($seen.Count -ne $expected.Count) { throw 'Runtime archive file set does not match lock.' }
} finally { $zip.Dispose() }
try {
    [IO.Compression.ZipFile]::ExtractToDirectory($archive, $Destination)
    foreach ($path in $expected.Keys) {
        if ((Get-FileHash -LiteralPath (Join-Path $Destination $path) -Algorithm SHA256).Hash -ine $expected[$path]) { throw "Runtime file hash mismatch: $path" }
    }
    foreach ($name in @('msedgewebview2.exe','msedge.dll','icudtl.dat','resources.pak')) {
        if (-not $expected.ContainsKey("runtime/$name")) { throw "Runtime missing required file: runtime/$name" }
    }
    (Resolve-Path -LiteralPath $Destination).Path
} catch {
    Remove-Item -LiteralPath $Destination -Recurse -Force -ErrorAction SilentlyContinue
    throw
}
