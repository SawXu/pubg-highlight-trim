param(
    [Parameter(Mandatory=$true)][string]$Bundle,
    [switch]$SkipCliScan,
    [switch]$VerboseScan
)
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot 'public_bundle_smoke.ps1')
$source = (Resolve-Path -LiteralPath $Bundle).Path
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("pubg-bundle-smoke-offline-assets-read-only-{0}" -f [guid]::NewGuid())
# Exercise the installation path as well as input paths with Unicode/spaces.
$bundleName = "bundle " + [string][char]0x6D4B + [string][char]0x8BD5
$copy = Join-Path $tempRoot $bundleName
# Exercise Unicode and spaces without relying on PowerShell source-file encoding.
$unicodeInputName = "input-" + [string][char]0x6D4B + [string][char]0x8BD5 + " clips"
$inputDir = Join-Path $tempRoot $unicodeInputName
$outputDir = Join-Path $tempRoot "output"
$oldTemp = $env:TEMP; $oldTmp = $env:TMP
$oldBackend = $env:PUBG_OCR_BACKEND; $oldProvider = $env:PUBG_OCR_PROVIDER
$oldDevice = $env:PUBG_OCR_DIRECTML_DEVICE_ID
$aclLocked = $false
$firewallRules = @()
$networkIsolation = ($env:GITHUB_ACTIONS -eq 'true')

function Assert-File([string]$Path, [string]$Purpose) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing ${Purpose}: $Path" }
}
function Verify-Models([string]$ModelDir) {
    $manifestPath = Join-Path $ModelDir "pp_ocrv6_medium.json"
    Assert-File $manifestPath "OCR model manifest"
    try { $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json } catch { throw "OCR model manifest is invalid JSON: $($_.Exception.Message)" }
    foreach ($role in @('detector','recognizer','dictionary')) {
        $entry = $manifest.$role
        if ($null -eq $entry -or [string]::IsNullOrWhiteSpace($entry.file)) { throw "OCR model manifest has no $role asset" }
        $path = Join-Path $ModelDir $entry.file
        Assert-File $path "$role model"
        $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne ([string]$entry.sha256).ToLowerInvariant()) { throw "$role sha256 mismatch: expected $($entry.sha256), actual $actual" }
        if ($role -eq 'dictionary') {
            try { $lines = @(Get-Content -LiteralPath $path -Encoding UTF8 | Where-Object { $_.Length -gt 0 }) } catch { throw "dictionary is not valid UTF-8: $($_.Exception.Message)" }
            if ($lines.Count -lt 100) { throw "dictionary content is unexpectedly small: $($lines.Count) symbols" }
        } else {
            $bytes = [IO.File]::ReadAllBytes($path)
            if ($bytes.Length -lt 8) { throw "$role is unexpectedly small to be an ONNX model" }
            # Full ONNX protobuf decoding and contract names are enforced by
            # cargo xtask verify during packaging; inference below validates that
            # the shipped bytes can actually be loaded by the Rust engine.
        }
    }
}
function Assert-DefaultProvider($Summary) {
    $providers = @($Summary.ocr_providers)
    if ($providers.Count -eq 0 -or $null -eq $providers[0]) { throw 'Default CLI scan produced no OCR provider diagnostics' }
    foreach ($provider in $providers) {
        if ($provider.provider_mode -ne 'auto' -or $provider.provider_requested -ne 'directml' -or $provider.device_id -ne 0) {
            throw "Default CLI must select mode=auto/requested=directml/device_id=0: $($provider | ConvertTo-Json -Compress)"
        }
        if ($provider.provider_actual -eq 'directml') {
            if (-not [string]::IsNullOrWhiteSpace([string]$provider.fallback_reason)) { throw 'Successful DirectML must not have a fallback reason' }
        } elseif ($provider.provider_actual -eq 'cpu') {
            if ([string]::IsNullOrWhiteSpace([string]$provider.fallback_reason) -or $provider.fallback_reason -notlike 'directml-*') {
                throw 'Default CPU fallback must explain the DirectML failure'
            }
        } else { throw "Default CLI did not perform real CPU/DirectML OCR: $($provider.provider_actual)" }
        # Only no-GPU CI declares this requirement. A local GPU may succeed.
        if ($env:PUBG_OCR_REQUIRE_CPU_FALLBACK -eq '1' -and $provider.provider_actual -ne 'cpu') {
            throw 'No-GPU assurance requires the default auto request to actually fall back to CPU'
        }
    }
}
function Invoke-JsonLine([string]$Executable, [string]$ModelDir, [string]$Provider = 'cpu') {
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $Executable
    $psi.Arguments = '"' + $ModelDir.Replace('"','\"') + '" 1 ' + $Provider
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    $previousInputEncoding = [Console]::InputEncoding
    try {
        # .NET Framework creates an AutoFlush StreamWriter during Start(). Its
        # inherited UTF-8 encoding can flush a BOM *before* BaseStream.Write.
        # Set the encoding before process creation, not only for our own bytes.
        [Console]::InputEncoding = New-Object System.Text.UTF8Encoding($false)
        if (-not $process.Start()) { throw "Could not start OCR engine (missing DLL or executable)" }
        $request = @{ pixels_b64 = [Convert]::ToBase64String([byte[]]::new(16*16*3)); width = 16; height = 16 } | ConvertTo-Json -Compress
        # PowerShell 5.1's text writer can emit a BOM (and inherit CP936),
        # which corrupts the first JSONL byte. Write BOM-free UTF-8 directly
        # to the redirected stream. Drain stdout/stderr concurrently so a
        # verbose engine cannot deadlock before WaitForExit.
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $utf8 = New-Object System.Text.UTF8Encoding($false)
        $inputBytes = $utf8.GetBytes($request + "`n")
        $inputStream = $process.StandardInput.BaseStream
        $inputStream.Write($inputBytes, 0, $inputBytes.Length)
        $inputStream.Flush()
        $inputStream.Close()
        $process.WaitForExit()
        $stdout = $stdoutTask.Result
        $stderr = $stderrTask.Result
        if ($process.ExitCode -ne 0) { throw "OCR engine inference failed (exit $($process.ExitCode)): $stderr" }
        $line = $stdout -split "`r?`n" | Where-Object { $_.Trim() } | Select-Object -First 1
        if (-not $line) { throw "OCR engine returned no inference response" }
        $row = $line | ConvertFrom-Json
        if ($Provider -eq 'cpu') {
            if ($row.status -ne 'ok' -or $row.provider -ne 'cpu') { throw "CPU OCR smoke requires status=ok/provider=cpu: $line" }
        } elseif ($Provider -eq 'directml') {
            $directMlOk = $row.status -eq 'ok' -and $row.provider -eq 'directml'
            $directMlFallback = $row.status -eq 'provider-fallback' -and $row.provider -eq 'cpu' -and -not [string]::IsNullOrWhiteSpace([string]$row.fallback_reason)
            if (-not ($directMlOk -or $directMlFallback)) { throw "DirectML OCR smoke requires ok/directml or provider-fallback/cpu with reason: $line" }
        } else { throw "Unsupported smoke provider: $Provider" }
    } finally {
        $process.Dispose()
        [Console]::InputEncoding = $previousInputEncoding
    }
}
try {
    New-Item -ItemType Directory -Path $copy,$inputDir,$outputDir -Force | Out-Null
    & robocopy $source $copy /E /COPY:DAT /R:1 /W:1 | Out-Host
    if ($LASTEXITCODE -gt 7) { throw "Bundle copy failed (robocopy exit $LASTEXITCODE)" }
    $exe = Join-Path $copy "pubg-highlight-trim.exe"
    $engine = Join-Path $copy "ocr_engine.exe"
    $models = Join-Path $copy "ocr_models"
    Assert-File $exe "Rust application executable"
    Assert-File $engine "OCR engine executable (issue #68/#69 integration is incomplete if absent)"
    Verify-Models $models
    $env:TEMP = $tempRoot; $env:TMP = $tempRoot
    $oldPath = $env:PATH
    $env:PATH = (Join-Path $copy "_internal\vendor\ffmpeg") + ";" + $env:PATH
    Remove-Item Env:PUBG_OCR_BACKEND -ErrorAction SilentlyContinue
    Remove-Item Env:PUBG_OCR_PROVIDER -ErrorAction SilentlyContinue
    Remove-Item Env:PUBG_OCR_DIRECTML_DEVICE_ID -ErrorAction SilentlyContinue

    # The shipped tree must remain read-only while model loading and inference run.
    $smokeUser = if ($env:USERDOMAIN) { "$env:USERDOMAIN\$env:USERNAME" } else { $env:USERNAME }
    $writeRights = [System.Security.AccessControl.FileSystemRights]::Write
    $inherit = [System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [System.Security.AccessControl.InheritanceFlags]::ObjectInherit
    $dirRule = [System.Security.AccessControl.FileSystemAccessRule]::new($smokeUser, $writeRights, $inherit, [System.Security.AccessControl.PropagationFlags]::None, [System.Security.AccessControl.AccessControlType]::Deny)
    $fileRule = [System.Security.AccessControl.FileSystemAccessRule]::new($smokeUser, $writeRights, [System.Security.AccessControl.AccessControlType]::Deny)
    $items = @((Get-Item $copy)) + @(Get-ChildItem $copy -Recurse -Force)
    foreach ($item in $items) {
        $acl = Get-Acl -LiteralPath $item.FullName
        $acl.AddAccessRule($(if ($item.PSIsContainer) { $dirRule } else { $fileRule }))
        Set-Acl -LiteralPath $item.FullName -AclObject $acl
    }
    $aclLocked = $true
    try { New-Item -ItemType File -Path (Join-Path $copy ".write-test") -ErrorAction Stop | Out-Null; throw "Read-only ACL allowed a write" } catch { if ($_.Exception.Message -eq "Read-only ACL allowed a write") { throw } }

    if ($networkIsolation) {
        if ($null -eq (Get-Command New-NetFirewallRule -ErrorAction SilentlyContinue)) { throw "New-NetFirewallRule is unavailable; cannot prove offline CI smoke" }
        $rulePrefix = "pubg-highlight-trim-smoke-$([guid]::NewGuid().ToString('N'))"
        foreach ($program in @($exe, $engine)) {
            $rule = New-NetFirewallRule -DisplayName "$rulePrefix-$([IO.Path]::GetFileName($program))" -Direction Outbound -Program $program -Action Block -Profile Any -ErrorAction Stop
            $firewallRules += $rule.Name
        }
        Write-Host "Offline CI smoke: outbound firewall rules installed for app and OCR engine."
    }

    & $exe --version | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Rust application --version failed" }
    # Exercise BOM-free JSONL under both common Windows console encodings.
    $oldConsoleEncoding = [Console]::OutputEncoding
    $oldConsoleInputEncoding = [Console]::InputEncoding
    try {
        foreach ($codePage in @(65001, 936)) {
            [Console]::OutputEncoding = [Text.Encoding]::GetEncoding($codePage)
            foreach ($bom in @($true, $false)) {
                [Console]::InputEncoding = New-Object System.Text.UTF8Encoding($bom)
                Invoke-JsonLine $engine $models
            }
        }
        # DirectML is optional: a missing DLL/GPU must produce an explicit CPU
        # fallback diagnostic, while the CPU provider remains fully usable.
        Invoke-JsonLine $engine $models 'directml'
    } finally {
        [Console]::OutputEncoding = $oldConsoleEncoding
        [Console]::InputEncoding = $oldConsoleInputEncoding
    }
    # A missing model must be an explicit engine failure, never a normal OCR miss.
    $missingModels = Join-Path $tempRoot "missing-models"
    New-Item -ItemType Directory -Path $missingModels -Force | Out-Null
    try { Invoke-JsonLine $engine $missingModels; throw "OCR engine unexpectedly succeeded with missing models" } catch { if ($_.Exception.Message -like "OCR engine unexpectedly succeeded*") { throw } }
    # A missing executable/DLL must be diagnosed at the process boundary.
    try { Invoke-JsonLine (Join-Path $tempRoot "missing-ocr-engine.exe") $models; throw "Missing OCR engine unexpectedly started" } catch { if ($_.Exception.Message -like "Missing OCR engine unexpectedly*") { throw } }

    if (-not $SkipCliScan) {
        $ffmpeg = Join-Path $copy "_internal\vendor\ffmpeg\ffmpeg.exe"
        Assert-File $ffmpeg "bundled ffmpeg.exe"
        $video = Join-Path $inputDir "smoke.mp4"
        $videoCopy = Join-Path $inputDir "smoke-copy.mp4"
        & $ffmpeg -hide_banner -loglevel error -f lavfi -i "color=c=black:s=320x240:d=1" -c:v mpeg4 -y $video
        if ($LASTEXITCODE -ne 0) { throw "Synthetic video creation failed" }
        Copy-Item -LiteralPath $video -Destination $videoCopy -Force
        $args = @('--files', $video, $videoCopy, '--output-dir', $outputDir, '--scan-only', '--no-brightness-gate', '--jobs', '2')
        if ($VerboseScan) { $args += '--verbose' }
        $scanOutput = @(& $exe @args 2>&1)
        $scanExit = $LASTEXITCODE
        $scanOutput | ForEach-Object { Write-Host $_ }
        if ($scanExit -ne 0 -and $scanExit -ne 2) { throw "Rust application CLI smoke failed (exit $scanExit)" }
        $summary = Get-ChildItem -LiteralPath $outputDir -Recurse -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -ieq 'summary.json' } | Select-Object -First 1
        $summaryData = $null
        if ($summary) {
            $summaryData = Get-Content $summary.FullName -Raw | ConvertFrom-Json
        } else {
            $summaryLine = $scanOutput | ForEach-Object { [string]$_ } | Where-Object { $_ -match '^SUMMARY\s+(.+)$' } | Select-Object -Last 1
            if ($summaryLine -and $summaryLine -match '^SUMMARY\s+(.+)$') { $summaryData = $Matches[1] | ConvertFrom-Json }
        }
        if ($null -eq $summaryData) { throw "Rust application CLI produced no summary.json or SUMMARY record; issue #69 is not integrated" }
        if ($summaryData.ocr_unavailable_reason) { throw "CLI scan OCR unavailable: $($summaryData.ocr_unavailable_reason)" }
        Assert-DefaultProvider $summaryData
        if (-not $VerboseScan -and ($scanOutput | Where-Object { [string]$_ -match 'OCR mode=' })) { throw 'Default non-verbose CLI leaked OCR provider diagnostics' }
        if (-not $summaryData.detectors -or $summaryData.detectors.PSObject.Properties.Count -eq 0 -or -not $summaryData.methods -or $summaryData.methods.PSObject.Properties.Count -eq 0) {
            throw "Rust application CLI has no real OCR inference result fields; #69 app integration is not complete"
        }
    }
    if (-not $SkipCliScan) {
        # These processes are still under the outbound firewall rules installed
        # above. A real public ROI exercises recognizer output and actual trim.
        $eventVideo = Join-Path $inputDir 'public-event.mp4'
        New-PublicSmokeVideo $ffmpeg $eventVideo
        $sourceHash = (Get-FileHash -LiteralPath $eventVideo -Algorithm SHA256).Hash
        $eventOutput = Join-Path $tempRoot 'event-output'
        $eventLog = @(& $exe --files $eventVideo --output-dir $eventOutput --roi '0,0,1,1' --game-lang zh-Hant --no-brightness-gate --min-event-sec 0 --coarse-step 1 --no-auto-candidate-csv --no-merge --before 1 --after 1 --jobs 1 2>&1)
        if ($LASTEXITCODE -ne 0) { throw "Offline public OCR-to-trim CLI failed (exit $LASTEXITCODE); OCR text is intentionally withheld" }
        $eventSummaryLine = $eventLog | ForEach-Object { [string]$_ } | Where-Object { $_ -match '^SUMMARY\s+' } | Select-Object -Last 1
        if (-not $eventSummaryLine) { throw "Offline public OCR-to-trim CLI returned no SUMMARY" }
        $eventSummary = ($eventSummaryLine -replace '^SUMMARY\s+', '') | ConvertFrom-Json
        Assert-DefaultProvider $eventSummary
        Assert-PublicSmokeResult $eventSummary $eventVideo $sourceHash $ffmpeg (Join-Path $copy '_internal\vendor\ffmpeg\ffprobe.exe')
        Write-Host 'Offline public-fixture OCR, legacy report, trim, decode, duration and source-hash checks passed.'
    }
    Write-Host "Windows bundle smoke OK (read-only, model hash/dictionary/inference and CLI scan; CI enforces outbound blocking)"
} finally {
    foreach ($rule in $firewallRules) { Remove-NetFirewallRule -Name $rule -ErrorAction SilentlyContinue }
    if ($aclLocked -and (Test-Path $copy)) { & icacls $copy /reset /T /C | Out-Null }
    $env:TEMP = $oldTemp; $env:TMP = $oldTmp
    if ($null -ne $oldPath) { $env:PATH = $oldPath }
    if ($null -eq $oldBackend) { Remove-Item Env:PUBG_OCR_BACKEND -ErrorAction SilentlyContinue } else { $env:PUBG_OCR_BACKEND = $oldBackend }
    if ($null -eq $oldProvider) { Remove-Item Env:PUBG_OCR_PROVIDER -ErrorAction SilentlyContinue } else { $env:PUBG_OCR_PROVIDER = $oldProvider }
    if ($null -eq $oldDevice) { Remove-Item Env:PUBG_OCR_DIRECTML_DEVICE_ID -ErrorAction SilentlyContinue } else { $env:PUBG_OCR_DIRECTML_DEVICE_ID = $oldDevice }
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
