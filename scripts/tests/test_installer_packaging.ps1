# Public packaging contract: one Native artifact, distribution metadata and exact ownership.
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$temp = Join-Path ([IO.Path]::GetTempPath()) ('installer packaging ' + [guid]::NewGuid().ToString())
function Write-Fixture([string]$Path, [string]$Value) {
    New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force | Out-Null
    [IO.File]::WriteAllText($Path, $Value, (New-Object Text.UTF8Encoding($false)))
}
try {
    $fakeRepo = Join-Path $temp 'repo with spaces'; $scripts = Join-Path $fakeRepo 'scripts'
    New-Item -ItemType Directory -Path $scripts -Force | Out-Null
    Copy-Item (Join-Path $repoRoot 'scripts\build_installers_windows.ps1'),(Join-Path $repoRoot 'scripts\collect_ui_licenses.mjs') -Destination $scripts
    $cli = Join-Path $fakeRepo 'dist\pubg-highlight-trim'
    foreach ($name in @('pubg-highlight-trim.exe','ocr_engine.exe','vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','msvcp140_1.dll','ocr_models\model.onnx','ocr_models\pp_ocrv6_medium.json','_internal\vendor\ffmpeg\ffmpeg.exe','_internal\vendor\ffmpeg\ffprobe.exe','licenses\inventory.json')) { Write-Fixture (Join-Path $cli $name) "fixture:$name" }
    Write-Fixture (Join-Path $fakeRepo 'dist\pubg-highlight-trim-native-ui\pubg-highlight-trim-native-ui.exe') 'fixture:native-ui'
    Write-Fixture (Join-Path $fakeRepo 'ui\package.json') '{"version":"1.2.3"}'
    foreach ($name in @('README.md','README_zh.md','LICENSE','THIRD_PARTY_NOTICES.md')) { Write-Fixture (Join-Path $fakeRepo $name) "fixture:$name" }
    $engine = Join-Path $temp 'observe-engine.ps1'
    Write-Fixture $engine @'
param([string]$Layout, [string]$OutputDirectory, [string]$Variant)
$ErrorActionPreference='Stop'
if ($Variant -cne 'Native') { throw 'Only Native installer is supported' }
$d=Get-Content (Join-Path $Layout 'distribution.json') -Raw|ConvertFrom-Json
if($d.schema -cne 'pubg-highlight-trim/distribution/v2' -or $d.artifact -cne 'native-ui' -or $d.variant -cne 'native' -or $d.version -cne '1.2.3'){throw 'Invalid native distribution contract'}
$m=Get-Content (Join-Path $Layout 'installer-manifest.json') -Raw|ConvertFrom-Json
if($m.schema -cne 'pubg-highlight-trim/installer-ownership/v1' -or $m.variant -cne 'native' -or $m.version -cne '1.2.3'){throw 'Invalid native ownership manifest'}
$actual=@(Get-ChildItem $Layout -Recurse -File|Where-Object Name -ne 'installer-manifest.json'|ForEach-Object{$_.FullName.Substring($Layout.Length).TrimStart('\','/').Replace('\','/')})
$paths=@($m.files.path)
if(@(Compare-Object $actual $paths).Count -ne 0 -or @($paths|Select-Object -Unique).Count -ne $paths.Count){throw 'Ownership does not enumerate exact packaged files'}
New-Item -ItemType Directory $OutputDirectory -Force|Out-Null
Set-Content (Join-Path $OutputDirectory 'Native-observed.txt') 'native contract observed'
'@
    $output = Join-Path $temp 'observations'
    & (Join-Path $scripts 'build_installers_windows.ps1') -Variant Native -SkipCliBuild -SkipUiBuild -InstallerEngine $engine -OutputDirectory $output
    if (-not (Test-Path (Join-Path $output 'Native-observed.txt'))) { throw 'Native engine contract not reached' }
    Write-Host 'Native packaging contract passed (not an installation lifecycle test).'
} finally { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
