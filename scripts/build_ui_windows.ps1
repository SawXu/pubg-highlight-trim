param(
    [switch]$SkipCliBuild,
    [ValidateSet('Release', 'Debug')][string]$Configuration = 'Release'
)

# Development-runnable layout only. Use build_installers_windows.ps1 for the
# System/Fixed installer entry point; this script never publishes a GUI ZIP.
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repoRoot
try {
    foreach ($command in @('cargo', 'npm.cmd')) {
        if ($null -eq (Get-Command $command -ErrorAction SilentlyContinue)) {
            throw "Required command '$command' is missing (Rust MSVC and Node.js are required)."
        }
    }
    if ($env:CARGO_TARGET_DIR -and -not [IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        throw 'CARGO_TARGET_DIR must be absolute when using this build script.'
    }
    if (-not $SkipCliBuild) {
        & powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1
        if ($LASTEXITCODE -ne 0) { throw 'CLI build failed' }
    }
    $cliBundle = Join-Path $repoRoot 'dist\pubg-highlight-trim'
    if (-not (Test-Path -LiteralPath "$cliBundle\pubg-highlight-trim.exe" -PathType Leaf)) {
        throw 'CLI bundle is missing. Build it first or omit -SkipCliBuild.'
    }
    & npm.cmd --prefix ui ci
    if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed' }
    Push-Location -LiteralPath (Join-Path $repoRoot 'ui')
    try {
        $arguments = @('run', 'tauri', '--', 'build', '--no-bundle')
        if ($Configuration -eq 'Debug') { $arguments += '--debug' }
        # Forward to Cargo (Tauri itself has no --locked flag).
        $arguments += @('--', '--locked')
        & npm.cmd @arguments
        if ($LASTEXITCODE -ne 0) { throw 'Tauri build failed' }
    }
    finally { Pop-Location }
    $profile = $Configuration.ToLowerInvariant()
    $targetRoot = Join-Path $repoRoot 'ui\src-tauri\target'
    if ($env:CARGO_TARGET_DIR) {
        $targetRoot = [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    }
    $executable = Join-Path $targetRoot "$profile\pubg-highlight-trim-ui.exe"
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw "Tauri executable is missing: $executable"
    }
    $layout = Join-Path $repoRoot 'dist\pubg-highlight-trim-ui'
    Remove-Item -LiteralPath $layout -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $layout -Force | Out-Null
    Copy-Item -LiteralPath $executable -Destination "$layout\pubg-highlight-trim-ui.exe"
    # Native UI imports resolve beside the UI executable, not inside cli/.
    foreach ($crtDll in @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'msvcp140_1.dll')) {
        $crtSource = Join-Path $cliBundle $crtDll
        if (-not (Test-Path -LiteralPath $crtSource -PathType Leaf)) { throw "Required UI CRT DLL is missing: $crtDll" }
        Copy-Item -LiteralPath $crtSource -Destination $layout
    }
    Copy-Item -LiteralPath $cliBundle -Destination "$layout\cli" -Recurse
    Copy-Item -LiteralPath README.md, README_zh.md, LICENSE, THIRD_PARTY_NOTICES.md -Destination $layout
    @'
PUBG Highlight Trim - Tauri development build
Run pubg-highlight-trim-ui.exe and keep the complete cli directory beside it.
Microsoft Edge WebView2 Runtime must already be installed. This layout does not
bundle WebView2, guarantee clean-host runtime dependencies, or provide an installer.
The configured UI data directory must be writable. See README.md / README_zh.md.
No Python installation is required by the CLI.
'@ | Set-Content -LiteralPath "$layout\RUN_UI.txt" -Encoding UTF8
    Write-Host "Built development layout: $layout (no release ZIP/installer or runtime self-containment claim)."
}
finally { Pop-Location }
