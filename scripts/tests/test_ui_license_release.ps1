# Fixture release entry -> engine output, not a real installer/runtime acceptance claim.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$temp = Join-Path ([IO.Path]::GetTempPath()) ('pht-ui-notices-'+[guid]::NewGuid().ToString('N'))
try {
    foreach ($dir in @('scripts','ui\native-ui','dist\pubg-highlight-trim\licenses','dist\pubg-highlight-trim-native-ui')) {
        New-Item -ItemType Directory -Path (Join-Path $temp $dir) -Force | Out-Null
    }
    Copy-Item (Join-Path $root 'scripts\build_installers_windows.ps1') (Join-Path $temp 'scripts')
    foreach ($name in @('README.md','README_zh.md','LICENSE')) {
        [IO.File]::WriteAllText((Join-Path $temp $name), "fixture:$name")
    }
    foreach ($name in @('pubg-highlight-trim.exe','vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','msvcp140_1.dll','licenses\inventory.json')) {
        [IO.File]::WriteAllText((Join-Path $temp "dist\pubg-highlight-trim\$name"), "fixture:$name")
    }
    [IO.File]::WriteAllText((Join-Path $temp 'dist\pubg-highlight-trim-native-ui\pubg-highlight-trim-native-ui.exe'), 'fixture:native-ui')
    [IO.File]::WriteAllText((Join-Path $temp 'ui\native-ui\Cargo.toml'), "[package]`nversion = `"1.0.0`"`n")

    $engine = Join-Path $temp 'engine.ps1'
    @'
param($Layout, $OutputDirectory, $Variant)
$ErrorActionPreference='Stop'
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
Copy-Item -LiteralPath $Layout -Destination (Join-Path $OutputDirectory 'product') -Recurse
'@ | Set-Content $engine -Encoding UTF8

    $builder = Join-Path $temp 'scripts\build_installers_windows.ps1'
    $output = Join-Path $temp 'output'

    # Fail closed when required root notice file is missing: engine must not publish output.
    $ErrorActionPreference = 'Continue'
    & powershell -NoProfile -ExecutionPolicy Bypass -File $builder -Variant Native -SkipCliBuild -SkipUiBuild -InstallerEngine $engine -OutputDirectory $output 2>&1 | Out-String | Out-Null
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -eq 0 -or (Test-Path $output)) { throw 'Missing notice allowed engine/product publication' }

    # Once required notices are present, native release staging succeeds and owns all shipped notice files.
    [IO.File]::WriteAllText((Join-Path $temp 'THIRD_PARTY_NOTICES.md'), 'Fixture third-party notices, not a real dependency license.')
    & powershell -NoProfile -ExecutionPolicy Bypass -File $builder -Variant Native -SkipCliBuild -SkipUiBuild -InstallerEngine $engine -OutputDirectory $output
    if ($LASTEXITCODE -ne 0) { throw 'Valid fixture release rejected' }

    $product = Join-Path $output 'product'
    $ownership = Get-Content (Join-Path $product 'installer-manifest.json') -Raw | ConvertFrom-Json
    $expectedNotices = @(
        @{ Source = (Join-Path $temp 'LICENSE'); Product = 'LICENSE' },
        @{ Source = (Join-Path $temp 'THIRD_PARTY_NOTICES.md'); Product = 'THIRD_PARTY_NOTICES.md' },
        @{ Source = (Join-Path $temp 'dist\pubg-highlight-trim\licenses\inventory.json'); Product = 'cli/licenses/inventory.json' }
    )
    foreach ($notice in $expectedNotices) {
        $productPath = Join-Path $product ($notice.Product.Replace('/', '\'))
        if (-not (Test-Path -LiteralPath $productPath -PathType Leaf)) { throw "Missing shipped notice: $($notice.Product)" }
        if ((Get-FileHash $productPath -Algorithm SHA256).Hash -ine (Get-FileHash $notice.Source -Algorithm SHA256).Hash) {
            throw "Product notice hash differs: $($notice.Product)"
        }
        if ($notice.Product -notin $ownership.files.path) {
            throw "Product notice is missing from installer ownership: $($notice.Product)"
        }
    }
    if (Test-Path -LiteralPath (Join-Path $product 'third_party\ui\UI_LICENSE_INVENTORY.json')) {
        throw 'Unexpected legacy Tauri UI license inventory in Native release product'
    }
    Write-Host 'Native UI license release-entry -> fixture product tests passed (not real NSIS/runtime acceptance).'
} finally { if (Test-Path $temp) { Remove-Item $temp -Recurse -Force } }
