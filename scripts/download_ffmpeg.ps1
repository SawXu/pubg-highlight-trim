[CmdletBinding()]
param(
    [string]$Destination = "vendor\ffmpeg",
    [string]$Url,
    [string]$Checksum,
    [string]$CacheDirectory = "vendor\ffmpeg-cache"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$lockPath = Join-Path $PSScriptRoot "ffmpeg-lock.json"
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
$defaultUrl = [string]$lock.url
$defaultChecksum = ([string]$lock.sha256).ToLowerInvariant()
$urlWasOverridden = $PSBoundParameters.ContainsKey("Url")
if ([string]::IsNullOrWhiteSpace($Url)) { $Url = $defaultUrl }
if ([string]::IsNullOrWhiteSpace($Checksum)) {
    if (-not $urlWasOverridden) { $Checksum = $defaultChecksum }
    else { throw "-Checksum is required when overriding -Url." }
}
$Checksum = $Checksum.Trim().ToLowerInvariant()
if ($Checksum -notmatch '^[0-9a-f]{64}$') { throw "Checksum must be a 64-character SHA-256 hex digest." }

$destPath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Destination))
$cachePath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $CacheDirectory))
$cacheName = [System.IO.Path]::GetFileName(([uri]$Url).AbsolutePath)
if ([string]::IsNullOrWhiteSpace($cacheName)) { $cacheName = "ffmpeg.zip" }
$zipPath = Join-Path $cachePath $cacheName
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("pubg-highlight-trim-ffmpeg-" + [guid]::NewGuid().ToString("N"))
$extractPath = Join-Path $tempRoot "extract"

try {
    New-Item -ItemType Directory -Force -Path $cachePath, $tempRoot, $extractPath | Out-Null
    if (Test-Path -LiteralPath $zipPath) {
        $actual = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $Checksum) {
            Write-Warning "Cached archive checksum mismatch; redownloading."
            Remove-Item -LiteralPath $zipPath -Force
        }
    }
    if (-not (Test-Path -LiteralPath $zipPath)) {
        Write-Host "Downloading FFmpeg from $Url"
        Invoke-WebRequest -Uri $Url -OutFile $zipPath
    }
    $actual = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Checksum) { throw "FFmpeg archive SHA-256 mismatch. Expected $Checksum, got $actual." }

    Expand-Archive -Path $zipPath -DestinationPath $extractPath -Force
    $binDir = Get-ChildItem -Path $extractPath -Recurse -Directory | Where-Object {
        (Test-Path (Join-Path $_.FullName "ffmpeg.exe")) -and (Test-Path (Join-Path $_.FullName "ffprobe.exe"))
    } | Select-Object -First 1
    if (-not $binDir) { throw "Could not find ffmpeg.exe and ffprobe.exe in downloaded archive." }

    $readmePath = Join-Path $destPath "README.md"
    $readmeText = if (Test-Path -LiteralPath $readmePath) { Get-Content -LiteralPath $readmePath -Raw } else { $null }
    if (Test-Path -LiteralPath $destPath) { Remove-Item -LiteralPath $destPath -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $destPath | Out-Null
    if ($null -ne $readmeText) { Set-Content -LiteralPath $readmePath -Value $readmeText -NoNewline }
    $runtimeFiles = Get-ChildItem -Path $binDir.FullName -File | Where-Object {
        $_.Name -in @("ffmpeg.exe", "ffprobe.exe") -or $_.Extension -ieq ".dll"
    }
    $runtimeFiles | Copy-Item -Destination $destPath -Force
    foreach ($name in @('ffmpeg.exe', 'ffprobe.exe')) {
        # Drain native stdout before selecting a line: Select-Object -First can
        # close the pipe early and turn a successful version probe into -1.
        $version = & (Join-Path $destPath $name) -version
        if ($LASTEXITCODE -ne 0) { throw "$name -version failed with exit code $LASTEXITCODE." }
        $version | Select-Object -First 1
    }
    Write-Host "FFmpeg runtime copied to $destPath"
}
finally {
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue }
}
