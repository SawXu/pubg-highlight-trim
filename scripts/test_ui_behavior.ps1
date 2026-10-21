param([switch]$SkipInstall)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repoRoot
try {
    if (-not $SkipInstall) {
        & npm.cmd --prefix ui ci
        if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed' }
    }
    & npm.cmd --prefix ui test
    if ($LASTEXITCODE -ne 0) { throw 'Frontend public behavior contracts failed' }
    & cargo test --locked --manifest-path ui/src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Shell public behavior contracts failed' }
    Write-Host 'Frontend and shell contracts passed. This does not certify interactive desktop UI-to-CLI or final package acceptance.'
}
finally { Pop-Location }
