param(
    [Parameter(Mandatory = $true)]
    [string]$Package
)

$ErrorActionPreference = "Stop"
$resolvedPackage = (Resolve-Path -LiteralPath $Package).Path
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("pubg-ui-verify-{0}" -f [guid]::NewGuid())
$extractDir = Join-Path $tempRoot "package"

try {
    New-Item -ItemType Directory -Path $extractDir -Force | Out-Null
    Expand-Archive -LiteralPath $resolvedPackage -DestinationPath $extractDir -Force

    $uiExecutable = Join-Path $extractDir "pubg-highlight-trim-ui.exe"
    $cliExecutable = Join-Path $extractDir "cli\pubg-highlight-trim.exe"
    if (-not (Test-Path -LiteralPath $uiExecutable -PathType Leaf)) {
        throw "The UI executable is missing from the final package."
    }
    if (-not (Test-Path -LiteralPath $cliExecutable -PathType Leaf)) {
        throw "The bundled CLI executable is missing from the final package."
    }

    & powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "verify_windows_dpi.ps1") -Executable $uiExecutable
    if ($LASTEXITCODE -ne 0) {
        throw "The final package UI executable failed DPI manifest verification."
    }
    Write-Host "Verified final UI package: $resolvedPackage"
}
finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
