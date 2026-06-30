param(
    [string]$Destination = "vendor\ffmpeg",
    [string]$Url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$destPath = Join-Path $repoRoot $Destination
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("pubg-highlight-trim-ffmpeg-" + [guid]::NewGuid().ToString("N"))
$zipPath = Join-Path $tempRoot "ffmpeg.zip"
$extractPath = Join-Path $tempRoot "extract"

New-Item -ItemType Directory -Force -Path $destPath, $tempRoot, $extractPath | Out-Null
Write-Host "Downloading FFmpeg from $Url"
Invoke-WebRequest -Uri $Url -OutFile $zipPath

Write-Host "Extracting FFmpeg"
Expand-Archive -Path $zipPath -DestinationPath $extractPath -Force
$binDir = Get-ChildItem -Path $extractPath -Recurse -Directory | Where-Object { Test-Path (Join-Path $_.FullName "ffmpeg.exe") -and Test-Path (Join-Path $_.FullName "ffprobe.exe") } | Select-Object -First 1
if (-not $binDir) {
    throw "Could not find ffmpeg.exe and ffprobe.exe in downloaded archive."
}

Get-ChildItem -Path $binDir.FullName -File | Copy-Item -Destination $destPath -Force
& (Join-Path $destPath "ffmpeg.exe") -version | Select-Object -First 1
& (Join-Path $destPath "ffprobe.exe") -version | Select-Object -First 1

Remove-Item -LiteralPath $tempRoot -Recurse -Force
Write-Host "FFmpeg runtime copied to $destPath"
