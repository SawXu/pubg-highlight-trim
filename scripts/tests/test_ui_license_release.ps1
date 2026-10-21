# Fixture release entry -> engine output, not a real installer/runtime acceptance claim.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$temp = Join-Path ([IO.Path]::GetTempPath()) ('pht-ui-notices-'+[guid]::NewGuid().ToString('N'))
try {
    foreach ($dir in @('scripts','ui\src-tauri\src','ui\node_modules\runtime','dist\pubg-highlight-trim','dist\pubg-highlight-trim-ui')) { New-Item -ItemType Directory -Path (Join-Path $temp $dir) -Force | Out-Null }
    Copy-Item (Join-Path $root 'scripts\build_installers_windows.ps1'),(Join-Path $root 'scripts\collect_ui_licenses.mjs') (Join-Path $temp 'scripts')
    foreach ($name in @('README.md','README_zh.md','LICENSE','THIRD_PARTY_NOTICES.md')) { [IO.File]::WriteAllText((Join-Path $temp $name), 'fixture') }
    foreach ($name in @('pubg-highlight-trim.exe','vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','msvcp140_1.dll')) { [IO.File]::WriteAllText((Join-Path $temp "dist\pubg-highlight-trim\$name"), 'fixture') }
    [IO.File]::WriteAllText((Join-Path $temp 'dist\pubg-highlight-trim-ui\pubg-highlight-trim-ui.exe'), 'fixture')
    [IO.File]::WriteAllText((Join-Path $temp 'ui\package.json'), '{"version":"1.0.0","dependencies":{"runtime":"1.0.0"}}')
    [IO.File]::WriteAllText((Join-Path $temp 'ui\package-lock.json'), '{"lockfileVersion":3,"packages":{"":{"dependencies":{"runtime":"1.0.0"}},"node_modules/runtime":{"version":"1.0.0","license":"MIT"}}}')
    [IO.File]::WriteAllText((Join-Path $temp 'ui\node_modules\runtime\package.json'), '{"name":"runtime","version":"1.0.0","license":"MIT"}')
    [IO.File]::WriteAllText((Join-Path $temp 'ui\src-tauri\Cargo.toml'), "[package]`nname = `"pubg-highlight-trim-ui`"`nversion = `"1.0.0`"`nedition = `"2021`"`n")
    [IO.File]::WriteAllText((Join-Path $temp 'ui\src-tauri\src\lib.rs'), '')
    & cargo generate-lockfile --offline --manifest-path (Join-Path $temp 'ui\src-tauri\Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Fixture lockfile creation failed' }
    $engine = Join-Path $temp 'engine.ps1'
    @'
param($Layout, $OutputDirectory, $Variant)
$ErrorActionPreference='Stop'
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
Copy-Item -LiteralPath $Layout -Destination (Join-Path $OutputDirectory 'product') -Recurse
'@ | Set-Content $engine -Encoding UTF8
    $builder = Join-Path $temp 'scripts\build_installers_windows.ps1'
    $output = Join-Path $temp 'output'
    $ErrorActionPreference = 'Continue'
    & powershell -NoProfile -ExecutionPolicy Bypass -File $builder -SkipCliBuild -SkipUiBuild -InstallerEngine $engine -OutputDirectory $output 2>&1 | Out-String | Out-Null
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -eq 0 -or (Test-Path $output)) { throw 'Missing notice allowed engine/product publication' }
    [IO.File]::WriteAllText((Join-Path $temp 'ui\node_modules\runtime\LICENSE'), 'Fixture license text, not a real dependency license.')
    & powershell -NoProfile -ExecutionPolicy Bypass -File $builder -SkipCliBuild -SkipUiBuild -InstallerEngine $engine -OutputDirectory $output
    if ($LASTEXITCODE -ne 0) { throw 'Valid fixture release rejected' }
    $product = Join-Path $output 'product'
    $inventory = Get-Content (Join-Path $product 'third_party\ui\UI_LICENSE_INVENTORY.json') -Raw | ConvertFrom-Json
    $ownership = Get-Content (Join-Path $product 'installer-manifest.json') -Raw | ConvertFrom-Json
    foreach ($notice in $inventory.components.notices) {
        $noticePath = Join-Path $product $notice.path
        if ((Get-FileHash $noticePath -Algorithm SHA256).Hash -ine $notice.sha256) { throw 'Product notice hash differs' }
        if ($notice.path -notin $ownership.files.path) { throw 'Product notice is missing from installer ownership' }
    }
    if ('third_party/ui/UI_LICENSE_INVENTORY.json' -notin $ownership.files.path) { throw 'Inventory missing from ownership' }
    Write-Host 'UI license release-entry -> fixture product tests passed (not real NSIS/runtime acceptance).'
} finally { if (Test-Path $temp) { Remove-Item $temp -Recurse -Force } }
