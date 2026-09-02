param(
    [switch]$SkipCliBuild,
    [string]$Configuration = "Release",
    [string]$Runtime = "win-x64"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

if (-not $SkipCliBuild) {
    & powershell -ExecutionPolicy Bypass -File "scripts\build_windows.ps1"
    if ($LASTEXITCODE -ne 0) { throw "CLI build failed with exit code $LASTEXITCODE" }
}

$cliBundle = "dist\pubg-highlight-trim"
if (-not (Test-Path -LiteralPath "$cliBundle\pubg-highlight-trim.exe")) {
    throw "CLI bundle is missing. Build it first or omit -SkipCliBuild."
}

$publishDir = "build\ui-publish"
$bundleDir = "dist\pubg-highlight-trim-ui"
$zipPath = "dist\pubg-highlight-trim-ui-windows-x64.zip"
Remove-Item -LiteralPath $publishDir, $bundleDir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue

dotnet publish "ui\PubgHighlightTrim.Ui\PubgHighlightTrim.Ui.csproj" `
    --configuration $Configuration `
    --runtime $Runtime `
    --self-contained true `
    --output $publishDir `
    -p:PublishSingleFile=true `
    -p:IncludeNativeLibrariesForSelfExtract=true `
    -p:EnableCompressionInSingleFile=true `
    -p:DebugType=None `
    -p:DebugSymbols=false
if ($LASTEXITCODE -ne 0) { throw "UI publish failed with exit code $LASTEXITCODE" }

& powershell -ExecutionPolicy Bypass -File "scripts\verify_windows_dpi.ps1" -Executable "$publishDir\pubg-highlight-trim-ui.exe"
if ($LASTEXITCODE -ne 0) { throw "DPI manifest verification failed" }

New-Item -ItemType Directory -Path $bundleDir | Out-Null
Copy-Item -LiteralPath "$publishDir\pubg-highlight-trim-ui.exe" -Destination "$bundleDir\pubg-highlight-trim-ui.exe" -Force
& powershell -ExecutionPolicy Bypass -File "scripts\verify_windows_dpi.ps1" -Executable "$bundleDir\pubg-highlight-trim-ui.exe"
if ($LASTEXITCODE -ne 0) { throw "Final bundle DPI manifest verification failed" }
Copy-Item -LiteralPath $cliBundle -Destination "$bundleDir\cli" -Recurse -Force
Copy-Item -LiteralPath "README.md", "README_zh.md", "LICENSE", "THIRD_PARTY_NOTICES.md" -Destination $bundleDir -Force
@"
PUBG Highlight Trim UI

Run pubg-highlight-trim-ui.exe. The bundled CLI is located in the cli folder and must remain beside the UI.
No Python or .NET installation is required.
"@ | Set-Content -LiteralPath "$bundleDir\RUN_UI.txt" -Encoding UTF8

Compress-Archive -Path "$bundleDir\*" -DestinationPath $zipPath -CompressionLevel Optimal -Force
$verifyPackage = Join-Path $repoRoot "scripts\verify_windows_ui_bundle.ps1"
& powershell -ExecutionPolicy Bypass -File $verifyPackage -Package $zipPath
if ($LASTEXITCODE -ne 0) { throw "Final UI package verification failed" }
$uiSizeMb = [math]::Round((Get-Item "$bundleDir\pubg-highlight-trim-ui.exe").Length / 1MB, 1)
$zipSizeMb = [math]::Round((Get-Item $zipPath).Length / 1MB, 1)
Write-Host "Built $zipPath (UI: $uiSizeMb MB, package: $zipSizeMb MB)"
