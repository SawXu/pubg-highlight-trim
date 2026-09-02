param(
    [Parameter(Mandatory = $true)]
    [string]$Executable
)

$ErrorActionPreference = "Stop"
$resolved = Resolve-Path -LiteralPath $Executable
$mt = Get-Command mt.exe -ErrorAction SilentlyContinue
if ($null -eq $mt) {
    throw "mt.exe is required to inspect the final executable manifest. Install the Windows SDK (CI and release machines must include it)."
}

$tempManifest = Join-Path ([IO.Path]::GetTempPath()) ("pubg-dpi-{0}.manifest" -f [guid]::NewGuid())
try {
    $inputResource = "$resolved;#1"
    & $mt.Source -nologo "-inputresource:$inputResource" "-out:$tempManifest"
    if ($LASTEXITCODE -ne 0) { throw "Could not extract the manifest from $resolved" }
    $manifest = Get-Content -LiteralPath $tempManifest -Raw
    if ($manifest -notmatch "<dpiAwareness[^>]*>\s*PerMonitorV2\s*</dpiAwareness>") {
        throw "The executable manifest does not declare PerMonitorV2 DPI awareness."
    }
    if ($manifest -notmatch "<dpiAware[^>]*>\s*true/pm\s*</dpiAware>") {
        throw "The executable manifest does not declare the legacy per-monitor DPI fallback."
    }
    Write-Host "Verified PerMonitorV2 manifest in $resolved"
}
finally {
    Remove-Item -LiteralPath $tempManifest -Force -ErrorAction SilentlyContinue
}
