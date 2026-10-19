param(
    [string]$ReleaseDirectory
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$buildScript = Join-Path $repoRoot 'scripts\build_rust_release.ps1'
if (-not $ReleaseDirectory) { $ReleaseDirectory = Join-Path $repoRoot 'rust\target\release' }
$ReleaseDirectory = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$temp = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())

function Assert-Rejected([scriptblock]$Action, [string]$ExpectedMessage) {
    try { & $Action } catch {
        if ($_.Exception.Message -notlike "*$ExpectedMessage*") { throw }
        Write-Host "Expected rejection: $ExpectedMessage"
        return
    }
    throw "Expected release reuse rejection: $ExpectedMessage"
}

New-Item -ItemType Directory -Path $temp | Out-Null
try {
    # Exercise the public verification seam on real release binaries. Never
    # mutate the build outputs consumed by subsequent assurance steps.
    foreach ($name in @('xtask.exe', 'ocr_engine.exe', 'pubg_highlight_trim.exe', 'release-build-manifest.json')) {
        Copy-Item -LiteralPath (Join-Path $ReleaseDirectory $name) -Destination $temp
    }
    & $buildScript -ReleaseDirectory $temp -VerifyOnly

    $manifestPath = Join-Path $temp 'release-build-manifest.json'
    $original = Get-Content -LiteralPath $manifestPath -Raw
    $manifest = $original | ConvertFrom-Json
    $manifest.commit = 'wrong-commit'
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
    Assert-Rejected { & $buildScript -ReleaseDirectory $temp -VerifyOnly } 'commit, toolchain or build configuration'

    $manifest = $original | ConvertFrom-Json
    $manifest.configuration_json = '{}'
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
    Assert-Rejected { & $buildScript -ReleaseDirectory $temp -VerifyOnly } 'commit, toolchain or build configuration'

    $original | Set-Content -LiteralPath $manifestPath -Encoding UTF8
    [IO.File]::AppendAllText((Join-Path $temp 'xtask.exe'), 'tamper')
    Assert-Rejected { & $buildScript -ReleaseDirectory $temp -VerifyOnly } 'hash mismatch'

    Remove-Item -LiteralPath $manifestPath
    Assert-Rejected { & $buildScript -ReleaseDirectory $temp -VerifyOnly } 'requires'
    Write-Host 'Release reuse contract tests passed.'
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force
}
