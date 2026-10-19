param(
    [string]$ReleaseDirectory,
    [switch]$VerifyOnly
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $ReleaseDirectory) { $ReleaseDirectory = Join-Path $repoRoot "rust\target\release" }
$ReleaseDirectory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ReleaseDirectory)
$manifestPath = Join-Path $ReleaseDirectory "release-build-manifest.json"
$binaryNames = @("pubg_highlight_trim.exe", "ocr_engine.exe", "xtask.exe")

# Cargo gives encoded flags precedence over RUSTFLAGS. Refuse that ambiguous
# configuration rather than silently omitting the required delay-load flags.
if ($env:CARGO_ENCODED_RUSTFLAGS) { throw "CARGO_ENCODED_RUSTFLAGS is not supported by the Windows release build; use RUSTFLAGS." }
if ($env:CARGO_BUILD_TARGET) { throw "Unset CARGO_BUILD_TARGET for the native Windows x64 release build." }
$oldRustFlags = $env:RUSTFLAGS
try {
    # Remove every existing copy before appending one canonical pair. This also
    # makes the CI path and a standalone bundle build use identical fingerprints.
    $flags = [regex]::Replace([string]$oldRustFlags, '(?i)(?:^|\s)-C\s*link-args=(?:/DELAYLOAD:directml\.dll|delayimp\.lib)(?=\s|$)', ' ')
    $flags = [regex]::Replace($flags, '\s+', ' ').Trim()
    $env:RUSTFLAGS = "$flags -C link-args=/DELAYLOAD:directml.dll -C link-args=delayimp.lib".Trim()

    Push-Location -LiteralPath $repoRoot
    try {
        $commit = (& git rev-parse HEAD | Out-String).Trim()
        if ($LASTEXITCODE -ne 0) { throw "Cannot identify release source commit" }
        $toolchain = (& rustc -vV | Out-String).Trim()
        if ($LASTEXITCODE -ne 0) { throw "Cannot identify Rust toolchain" }
        if ($toolchain -notmatch '(?m)^host: x86_64-pc-windows-msvc\r?$') { throw "Windows releases require the native x86_64-pc-windows-msvc toolchain" }
        $inputs = [ordered]@{}
        foreach ($path in @('rust/Cargo.lock', 'rust/Cargo.toml', 'rust/ocr_engine/Cargo.toml', 'rust/xtask/Cargo.toml', 'scripts/build_rust_release.ps1', '.cargo/config', '.cargo/config.toml', 'rust/.cargo/config', 'rust/.cargo/config.toml')) {
            if (Test-Path -LiteralPath $path -PathType Leaf) {
                $inputs[$path] = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        }
        $buildEnvironment = [ordered]@{}
        Get-ChildItem Env: | Where-Object { $_.Name -match '^(CARGO_PROFILE_RELEASE_|RUSTC$|RUSTC_WRAPPER$|RUSTC_WORKSPACE_WRAPPER$|CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_)' } | Sort-Object Name | ForEach-Object {
            $buildEnvironment[$_.Name] = $_.Value
        }
        $configuration = [ordered]@{
            profile = 'release'
            features = 'cli,directml'
            binaries = $binaryNames
            rustflags = $env:RUSTFLAGS
            toolchain = $toolchain
            inputs = $inputs
            environment = $buildEnvironment
        }
        $configurationJson = ConvertTo-Json -InputObject $configuration -Depth 8 -Compress
        if ($VerifyOnly) {
            if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "Release reuse requires $manifestPath" }
            $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
            if ($manifest.schema -cne 'pubg-highlight-trim/release-build/v1' -or $manifest.commit -cne $commit -or $manifest.configuration_json -cne $configurationJson) {
                throw "Release manifest does not match the current commit, toolchain or build configuration"
            }
            foreach ($name in $binaryNames) {
                $entry = @($manifest.files | Where-Object { $_.name -ceq $name })
                $path = Join-Path $ReleaseDirectory $name
                if ($entry.Count -ne 1 -or -not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing release binary/hash: $name" }
                $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
                if ($actual -cne $entry[0].sha256) { throw "Release binary hash mismatch: $name" }
            }
            Write-Host "Verified release binaries for commit $commit in $ReleaseDirectory"
        } else {
            # Never leave an old attestation usable after a failed build.
            Remove-Item -LiteralPath $manifestPath -Force -ErrorAction SilentlyContinue
            & cargo build --locked --manifest-path rust\Cargo.toml --target-dir rust\target --release --features cli,directml --bin pubg_highlight_trim --bin ocr_engine --bin xtask --timings
            if ($LASTEXITCODE -ne 0) { throw "Rust release build failed with exit code $LASTEXITCODE" }
            New-Item -ItemType Directory -Path $ReleaseDirectory -Force | Out-Null
            $files = foreach ($name in $binaryNames) {
                $source = Join-Path $repoRoot "rust\target\release\$name"
                $destination = Join-Path $ReleaseDirectory $name
                if ($source -ine $destination) { Copy-Item -LiteralPath $source -Destination $destination -Force }
                [ordered]@{ name = $name; sha256 = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant() }
            }
            [ordered]@{
                schema = 'pubg-highlight-trim/release-build/v1'
                commit = $commit
                configuration_json = $configurationJson
                files = @($files)
            } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
            Write-Host "Built release binaries and integrity manifest: $manifestPath"
        }
    } finally { Pop-Location }
} finally {
    if ($null -eq $oldRustFlags) { Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue }
    else { $env:RUSTFLAGS = $oldRustFlags }
}
