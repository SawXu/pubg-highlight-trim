# Public packaging contract only. The engine stub observes staging inputs; it
# does not simulate installation and is not a real Windows lifecycle test.
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$temp = Join-Path ([IO.Path]::GetTempPath()) ('installer packaging ' + [guid]::NewGuid().ToString())
function Write-Fixture([string]$Path, [string]$Value) {
    New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force | Out-Null
    [IO.File]::WriteAllText($Path, $Value, (New-Object Text.UTF8Encoding($false)))
}
try {
    $fakeRepo = Join-Path $temp 'repo with spaces'
    $scripts = Join-Path $fakeRepo 'scripts'
    New-Item -ItemType Directory -Path $scripts -Force | Out-Null
    $entry = Join-Path $scripts 'build_installers_windows.ps1'
    Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\build_installers_windows.ps1') -Destination $entry
    Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts\collect_ui_licenses.mjs') -Destination $scripts
    $cli = Join-Path $fakeRepo 'dist\pubg-highlight-trim'
    foreach ($name in @('pubg-highlight-trim.exe','ocr_engine.exe','vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','msvcp140_1.dll','ocr_models\model.onnx','ocr_models\pp_ocrv6_medium.json','_internal\vendor\ffmpeg\ffmpeg.exe','_internal\vendor\ffmpeg\ffprobe.exe','licenses\inventory.json')) {
        Write-Fixture (Join-Path $cli $name) "fixture:$name"
    }
    Write-Fixture (Join-Path $fakeRepo 'dist\pubg-highlight-trim-ui\pubg-highlight-trim-ui.exe') 'fixture:ui'
    Write-Fixture (Join-Path $fakeRepo 'ui\package.json') '{"version":"1.2.3"}'
    Write-Fixture (Join-Path $fakeRepo 'ui\package-lock.json') '{"lockfileVersion":3,"packages":{"":{}}}'
    Write-Fixture (Join-Path $fakeRepo 'ui\src-tauri\Cargo.toml') "[package]`nname = `"pubg-highlight-trim-ui`"`nversion = `"1.2.3`"`nedition = `"2021`"`n"
    Write-Fixture (Join-Path $fakeRepo 'ui\src-tauri\src\lib.rs') ''
    & cargo generate-lockfile --offline --manifest-path (Join-Path $fakeRepo 'ui\src-tauri\Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Cargo lock failed' }
    foreach ($name in @('README.md','README_zh.md','LICENSE','THIRD_PARTY_NOTICES.md')) { Write-Fixture (Join-Path $fakeRepo $name) "fixture:$name" }
    $runtime = Join-Path $temp 'trusted runtime'
    foreach ($name in @('msedgewebview2.exe','msedge.dll','icudtl.dat','resources.pak','locales\en-US.pak')) { Write-Fixture (Join-Path $runtime $name) "runtime:$name" }
    $hash = (Get-FileHash -LiteralPath (Join-Path $runtime 'msedgewebview2.exe') -Algorithm SHA256).Hash
    $license = Join-Path $temp 'fixed-runtime-license.json'
    Write-Fixture (Join-Path $temp 'fixture-terms.txt') 'TEST FIXTURE ONLY, NOT MICROSOFT TERMS'
    $runtimeFiles = @(Get-ChildItem $runtime -Recurse -File | ForEach-Object { @{ path=$_.FullName.Substring($runtime.Length).TrimStart('\','/').Replace('\','/'); sha256=(Get-FileHash $_.FullName -Algorithm SHA256).Hash } })
    @{ schema='pubg-highlight-trim/fixed-runtime-license/v1'; version='fixture'; source='https://learn.microsoft.com/example-fixture'; reviewed_by='test fixture'; obligations='test-only not legal approval'; runtime_files=$runtimeFiles; notices=@(@{path='fixture-terms.txt'; source='https://www.microsoft.com/example-fixture'; sha256=(Get-FileHash (Join-Path $temp 'fixture-terms.txt') -Algorithm SHA256).Hash}) } | ConvertTo-Json -Depth 8 | Set-Content $license -Encoding UTF8
    $licenseHash = (Get-FileHash $license -Algorithm SHA256).Hash
    $engine = Join-Path $temp 'observe-engine.ps1'
    Write-Fixture $engine @'
param([string]$Layout, [string]$OutputDirectory, [string]$Variant)
$ErrorActionPreference = 'Stop'
$distributionPath = Join-Path $Layout 'distribution.json'
$bytes = [IO.File]::ReadAllBytes($distributionPath)
if ($bytes.Length -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191) { throw 'distribution.json must not have a UTF-8 BOM' }
$distribution = Get-Content -LiteralPath $distributionPath -Raw | ConvertFrom-Json
$expectedVersion = if ($Variant -eq 'System') { '1.2.3' } else { '2.3.4' }
if ($distribution.variant -cne $Variant.ToLowerInvariant() -or $distribution.minimum_webview2_major -ne 120 -or $distribution.version -cne $expectedVersion) { throw 'Invalid distribution contract/version' }
$required = @('third_party/ui/UI_LICENSE_INVENTORY.json','pubg-highlight-trim-ui.exe','distribution.json','README.md','README_zh.md','LICENSE','THIRD_PARTY_NOTICES.md','vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','msvcp140_1.dll','cli/pubg-highlight-trim.exe','cli/ocr_engine.exe','cli/ocr_models/model.onnx','cli/ocr_models/pp_ocrv6_medium.json','cli/_internal/vendor/ffmpeg/ffmpeg.exe','cli/_internal/vendor/ffmpeg/ffprobe.exe','cli/licenses/inventory.json')
foreach ($dll in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','msvcp140_1.dll')) { $required += "cli/$dll" }
if ($Variant -eq 'Fixed') {
    foreach ($name in @('msedgewebview2.exe','msedge.dll','icudtl.dat','resources.pak','locales/en-US.pak')) { $required += "WebView2Runtime/$name" }
} elseif (Test-Path -LiteralPath (Join-Path $Layout 'WebView2Runtime')) { throw 'System must not bundle Fixed runtime' }
$manifest = Get-Content -LiteralPath (Join-Path $Layout 'installer-manifest.json') -Raw | ConvertFrom-Json
if ($manifest.schema -cne 'pubg-highlight-trim/installer-ownership/v1' -or $manifest.variant -cne $distribution.variant -or $manifest.version -cne $expectedVersion) { throw 'Invalid ownership schema/variant/version' }
$paths = @($manifest.files | ForEach-Object {
    if ($_.owner -cne 'application' -or $_.action -cne 'replace-or-remove') { throw 'Invalid ownership record' }
    $_.path
})
foreach ($path in $required) {
    if (-not (Test-Path -LiteralPath (Join-Path $Layout $path) -PathType Leaf)) { throw "Missing packaged resource: $path" }
    if ($path -cnotin $paths) { throw "Missing ownership: $path" }
}
$actual = @(Get-ChildItem -LiteralPath $Layout -Recurse -File | Where-Object Name -ne 'installer-manifest.json' | ForEach-Object { $_.FullName.Substring($Layout.Length).TrimStart('\','/').Replace('\','/') })
if (@(Compare-Object $actual $paths).Count -ne 0 -or @($paths | Select-Object -Unique).Count -ne $paths.Count) { throw 'Ownership must enumerate exactly the packaged files once' }
if (@(Get-ChildItem -LiteralPath $Layout -Recurse -Filter distribution.json).Count -ne 1) { throw 'distribution.json must only occur at layout root' }
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
Set-Content -LiteralPath (Join-Path $OutputDirectory "$Variant-observed.txt") -Value 'public engine contract observed'
'@
    $output = Join-Path $temp 'observations'
    & $entry -Variant System -SkipCliBuild -SkipUiBuild -InstallerEngine $engine -OutputDirectory $output
    & $entry -Variant Fixed -Version '2.3.4' -SkipCliBuild -SkipUiBuild -InstallerEngine $engine -OutputDirectory $output -FixedRuntimePath $runtime -FixedRuntimeSha256 $hash -FixedRuntimeLicenseManifest $license -FixedRuntimeLicenseManifestSha256 $licenseHash
    foreach ($invalidVersion in @('', '1.2', '01.2.3', '1.2.3-beta', '1.2.3.4', 'not-a-version')) {
        $rejected = $false
        try { & $entry -Version $invalidVersion -SkipCliBuild -SkipUiBuild -InstallerEngine $engine -OutputDirectory $output } catch {
            if ($_.Exception.Message -notlike '*numeric semantic version*') { throw }
            $rejected = $true
        }
        if (-not $rejected) { throw "Invalid version accepted: $invalidVersion" }
    }
    foreach ($variant in @('System','Fixed')) {
        if (-not (Test-Path -LiteralPath (Join-Path $output "$variant-observed.txt"))) { throw "Engine contract not reached: $variant" }
    }
    if (@(Get-ChildItem -LiteralPath $fakeRepo -Recurse -Filter '*.zip').Count -ne 0) { throw 'Packaging must not produce a GUI ZIP' }
    Write-Host 'Public System/Fixed packaging contracts passed (not an installation lifecycle test).'
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}
