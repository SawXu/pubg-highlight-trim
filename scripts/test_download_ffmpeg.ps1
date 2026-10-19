# Public downloader seam: a real pinned cold download, offline warm reuse and
# rejection of a corrupt cache without damaging the installed runtime.
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$download = Join-Path $PSScriptRoot 'download_ffmpeg.ps1'
$lock = Get-Content (Join-Path $PSScriptRoot 'ffmpeg-lock.json') -Raw | ConvertFrom-Json
$relative = '.scratch\ffmpeg-contract-' + [guid]::NewGuid().ToString('N')
$cache = "$relative\cache"
$destination = "$relative\runtime"
try {
    & $download -CacheDirectory $cache -Destination $destination
    $binary = Join-Path $repoRoot "$destination\ffmpeg.exe"
    $expectedHash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
    # A loopback port with no service must not be contacted on a valid hit.
    $offlineUrl = 'http://127.0.0.1:1/' + $lock.asset
    & $download -CacheDirectory $cache -Destination $destination -Url $offlineUrl -Checksum $lock.sha256
    if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash -ne $expectedHash) { throw 'Warm reuse changed the binary.' }
    $archive = Join-Path $repoRoot "$cache\$($lock.asset)"
    [IO.File]::WriteAllText($archive, 'corrupt')
    $rejected = $false
    try { & $download -CacheDirectory $cache -Destination $destination -Url $offlineUrl -Checksum $lock.sha256 } catch { $rejected = $true }
    if (-not $rejected) { throw 'Corrupt archive was accepted without downloading a replacement.' }
    if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash -ne $expectedHash) { throw 'Failed replacement damaged the installed runtime.' }
    $rejected = $false
    try { & $download -CacheDirectory $cache -Destination $destination -Url $offlineUrl } catch { $rejected = $_.Exception.Message -like '*Checksum is required*' }
    if (-not $rejected) { throw 'URL override without checksum was accepted.' }
    Write-Host 'FFmpeg cold/warm/corruption contract passed.'
} finally {
    Remove-Item -LiteralPath (Join-Path $repoRoot $relative) -Recurse -Force -ErrorAction SilentlyContinue
}
