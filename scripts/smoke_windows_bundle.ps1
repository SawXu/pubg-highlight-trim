param(
    [Parameter(Mandatory=$true)][string]$Bundle,
    [switch]$SkipCliScan,
    [switch]$VerboseScan
)
$ErrorActionPreference = "Stop"
$source = (Resolve-Path -LiteralPath $Bundle).Path
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("pubg-bundle-smoke-offline-assets-read-only-{0}" -f [guid]::NewGuid())
$copy = Join-Path $tempRoot "bundle"
$inputDir = Join-Path $tempRoot "input"
$outputDir = Join-Path $tempRoot "output"
$oldTemp = $env:TEMP; $oldTmp = $env:TMP
$oldBackend = $env:PUBG_OCR_BACKEND; $oldProvider = $env:PUBG_OCR_PROVIDER; $oldEngine = $env:PUBG_OCR_ENGINE
$aclLocked = $false
try {
    New-Item -ItemType Directory -Path $copy,$inputDir,$outputDir -Force | Out-Null
    & robocopy $source $copy /E /COPY:DAT /R:1 /W:1 | Out-Host
    if ($LASTEXITCODE -gt 7) { throw "Bundle copy failed (robocopy exit $LASTEXITCODE)" }
    $exe = Join-Path $copy "pubg-highlight-trim.exe"
    $engine = Join-Path $copy "ocr_engine.exe"
    $models = Join-Path $copy "ocr_models"
    foreach ($path in @($exe, $engine, (Join-Path $models "det.onnx"), (Join-Path $models "rec.onnx"), (Join-Path $models "dict.txt"))) {
        if (-not (Test-Path $path -PathType Leaf)) { throw "Missing bundle file: $path" }
    }
    $env:TEMP = $tempRoot; $env:TMP = $tempRoot
    # The default backend must be exercised; do not force it through an environment override.
    Remove-Item Env:PUBG_OCR_BACKEND -ErrorAction SilentlyContinue
    Remove-Item Env:PUBG_OCR_PROVIDER -ErrorAction SilentlyContinue
    # Model verification and execution must work while the shipped tree is read-only.
    $smokeUser = if ($env:USERDOMAIN) { "$env:USERDOMAIN\$env:USERNAME" } else { $env:USERNAME }
    # Use .NET ACL APIs: icacls localized rights tokens vary and can reject valid-looking names.
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
    & $exe --version | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "CLI --version failed" }
    $verify = Get-Command python -ErrorAction SilentlyContinue
    if ($null -ne $verify) {
        & $verify.Source scripts\verify_ocr_models.py --model-dir $models
        if ($LASTEXITCODE -ne 0) { throw "Offline model verification failed" }
    }
    # Rust CLI JSONL smoke, with an explicit missing-engine failure check.
    $requestJson = '{"pixels_b64":"' + ([Convert]::ToBase64String([byte[]](0..767 | ForEach-Object { 0 }))) + '","width":16,"height":16}'
    # GitHub's Windows PowerShell profile can set $OutputEncoding to UTF-8 with
    # BOM. Write directly to stdin to keep the JSONL protocol BOM-free.
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $engine
    $startInfo.Arguments = '"{0}" 1 cpu' -f $models
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardInput = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    try {
        $null = $process.Start()
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        # StandardInputEncoding is unavailable on Windows PowerShell's .NET
        # Framework. Write bytes to BaseStream instead of using its text writer.
        $requestBytes = (New-Object System.Text.UTF8Encoding($false)).GetBytes($requestJson + "`n")
        $process.StandardInput.BaseStream.Write($requestBytes, 0, $requestBytes.Length)
        $process.StandardInput.BaseStream.Flush()
        $process.StandardInput.Close()
        if (-not $process.WaitForExit(120000)) {
            $process.Kill()
            throw "Rust OCR CLI smoke timed out"
        }
        $result = $stdoutTask.GetAwaiter().GetResult()
        $engineLog = $stderrTask.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "Rust OCR CLI smoke failed: $engineLog" }
    } finally {
        $process.Dispose()
    }
    $json = $result -split '\r?\n' | Where-Object { $_ -match '^\s*\{' } | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($json)) { throw "Rust OCR produced no JSON response: $result" }
    $response = $json | ConvertFrom-Json
    if ($response.status -ne "ok") { throw "Rust OCR blank ROI expected ok, got $($response.status)" }
    if ($response.provider -ne "cpu") { throw "Rust OCR expected cpu provider, got $($response.provider)" }
    if (-not $SkipCliScan) {
        $ffmpeg = Join-Path $copy "_internal\vendor\ffmpeg\ffmpeg.exe"
        if (-not (Test-Path $ffmpeg)) { throw "Bundled ffmpeg.exe is missing; cannot run CLI scan smoke" }
        $video = Join-Path $inputDir "smoke.mp4"
        & $ffmpeg -hide_banner -loglevel error -f lavfi -i "color=c=black:s=320x240:d=1" -c:v mpeg4 -y $video
        if ($LASTEXITCODE -ne 0) { throw "Synthetic video creation failed" }
        Copy-Item $video (Join-Path $inputDir "smoke-copy.mp4") -Force
        $previousErrorAction = $ErrorActionPreference; $ErrorActionPreference = "Continue"
        & $exe --files $video (Join-Path $inputDir "smoke-copy.mp4") --output-dir $outputDir --scan-only --scan-mode full --coarse-step 1 --no-auto-candidate-csv --no-merge --jobs 2 --no-brightness-gate $(if ($VerboseScan) { "--verbose" } else { "" }) -y
        $scanExit = $LASTEXITCODE
        if ($scanExit -ne 0 -and $scanExit -ne 2) { throw "CLI Rust OCR scan smoke failed (exit $scanExit)" }
        $env:PUBG_OCR_ENGINE = (Join-Path $tempRoot "missing-ocr-engine.exe")
        & $exe --files $video --output-dir (Join-Path $tempRoot "missing-output") --scan-only --scan-mode full --coarse-step 1 --no-auto-candidate-csv --no-merge --jobs 1 --no-brightness-gate -y *> $null
        $missingExit = $LASTEXITCODE
        Remove-Item Env:PUBG_OCR_ENGINE -ErrorAction SilentlyContinue
        $ErrorActionPreference = $previousErrorAction
        if ($missingExit -eq 0) { throw "CLI unexpectedly succeeded with missing PUBG_OCR_ENGINE" }
        $reports = Get-ChildItem $outputDir -Recurse -File -ErrorAction SilentlyContinue
        if (-not $reports) { throw "CLI scan produced no report files" }
        $summary = $reports | Where-Object Name -eq 'summary.json' | Select-Object -First 1
        if (-not $summary) { throw "CLI scan produced no summary.json" }
        $summaryData = Get-Content $summary.FullName -Raw | ConvertFrom-Json
        if ($summaryData.ocr_unavailable_reason) { throw "CLI scan OCR unavailable: $($summaryData.ocr_unavailable_reason)" }
        if ([int]$summaryData.source_count -ne 2) { throw "CLI scan expected two inputs, got $($summaryData.source_count)" }
        if (-not $summaryData.detectors -or $summaryData.detectors.PSObject.Properties.Count -eq 0) { throw "CLI scan did not perform OCR calls" }
        if (-not $summaryData.methods -or $summaryData.methods.PSObject.Properties.Count -eq 0) { throw "CLI scan produced no OCR method result" }
    }
    Write-Host "Windows bundle smoke OK (offline/read-only, Rust OCR CLI and scan)"
} finally {
    if ($aclLocked -and (Test-Path $copy)) { & icacls $copy /reset /T /C | Out-Null }
    $env:TEMP = $oldTemp; $env:TMP = $oldTmp
    if ($null -eq $oldBackend) { Remove-Item Env:PUBG_OCR_BACKEND -ErrorAction SilentlyContinue } else { $env:PUBG_OCR_BACKEND = $oldBackend }
    if ($null -eq $oldProvider) { Remove-Item Env:PUBG_OCR_PROVIDER -ErrorAction SilentlyContinue } else { $env:PUBG_OCR_PROVIDER = $oldProvider }
    if ($null -eq $oldEngine) { Remove-Item Env:PUBG_OCR_ENGINE -ErrorAction SilentlyContinue } else { $env:PUBG_OCR_ENGINE = $oldEngine }
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
