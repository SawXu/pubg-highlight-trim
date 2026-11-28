$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $repoRoot
try {
    & cargo test --locked --manifest-path ui/core/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Shared core public behavior contracts failed' }
    & cargo test --locked --manifest-path ui/native-ui/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Native UI behavior contracts failed' }
    Write-Host 'Core and native contracts passed. This does not certify interactive desktop UI-to-CLI or final package acceptance.'
}
finally { Pop-Location }
