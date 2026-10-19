param(
    [Parameter(Mandatory = $true)][string]$Package
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'public_bundle_smoke.ps1')
$resolvedPackage = (Resolve-Path -LiteralPath $Package).Path
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('pubg-ui-verify-{0}' -f [guid]::NewGuid())
$extractDir = Join-Path $tempRoot 'package'
$firewallRules = @()
$gui = $null
$oldProvider = $env:PUBG_OCR_PROVIDER
$oldPath = $env:PATH

function Find-UiControl([object]$Window, [string]$Id) {
    $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)
    $control = $Window.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $control) { throw "GUI UIAutomation control is unavailable: $Id (session $([Diagnostics.Process]::GetCurrentProcess().SessionId), interactive=$([Environment]::UserInteractive)); no silent UI skip is permitted" }
    return $control
}
function Set-UiValue([object]$Window, [string]$Id, [string]$Value) {
    $control = Find-UiControl $Window $Id
    $pattern = $control.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    $pattern.SetValue($Value)
}
function Set-UiToggle([object]$Window, [string]$Id, [bool]$Value) {
    $pattern = (Find-UiControl $Window $Id).GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
    $desired = if ($Value) { [Windows.Automation.ToggleState]::On } else { [Windows.Automation.ToggleState]::Off }
    if ($pattern.Current.ToggleState -ne $desired) { $pattern.Toggle() }
    if ($pattern.Current.ToggleState -ne $desired) { throw "GUI checkbox did not change: $Id" }
}
function Select-UiIndex([object]$Window, [string]$Id, [int]$Index) {
    $combo = Find-UiControl $Window $Id
    $expand = $combo.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern)
    $expand.Expand()
    $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem)
    $items = $combo.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
    if ($items.Count -le $Index) { throw "GUI selection items are unavailable: $Id" }
    $items[$Index].GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
    $expand.Collapse()
}
try {
    New-Item -ItemType Directory -Path $extractDir -Force | Out-Null
    Expand-Archive -LiteralPath $resolvedPackage -DestinationPath $extractDir -Force
    $uiExecutable = Join-Path $extractDir 'pubg-highlight-trim-ui.exe'
    $cliExecutable = Join-Path $extractDir 'cli\pubg-highlight-trim.exe'
    $engine = Join-Path $extractDir 'cli\ocr_engine.exe'
    foreach ($path in @($uiExecutable, $cliExecutable, $engine)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw 'A required UI/CLI executable is missing from the final package.' }
    }
    & powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'verify_windows_dpi.ps1') -Executable $uiExecutable
    if ($LASTEXITCODE -ne 0) { throw 'The final package UI executable failed DPI manifest verification.' }
    & powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke_windows_bundle.ps1') -Bundle (Join-Path $extractDir 'cli')
    if ($LASTEXITCODE -ne 0) { throw "The final UI package's bundled CLI failed offline OCR-to-trim verification." }

    # Launch the actual self-contained executable from the final ZIP, not dotnet
    # or a build-tree UI. On hosted CI block GUI and its CLI/OCR subprocesses.
    if ($env:GITHUB_ACTIONS -eq 'true') {
        $prefix = 'pubg-ui-smoke-' + [guid]::NewGuid().ToString('N')
        foreach ($program in @($uiExecutable, $cliExecutable, $engine)) {
            $rule = New-NetFirewallRule -DisplayName "$prefix-$([IO.Path]::GetFileName($program))" -Direction Outbound -Program $program -Action Block -Profile Any -ErrorAction Stop
            $firewallRules += $rule.Name
        }
    }
    $ffmpeg = Join-Path $extractDir 'cli\_internal\vendor\ffmpeg\ffmpeg.exe'
    $ffprobe = Join-Path $extractDir 'cli\_internal\vendor\ffmpeg\ffprobe.exe'
    $eventVideo = Join-Path $tempRoot 'public GUI event.mp4'
    $output = Join-Path $tempRoot 'GUI output'
    New-PublicSmokeVideo $ffmpeg $eventVideo -GuiLayout
    $sourceHash = (Get-FileHash -LiteralPath $eventVideo -Algorithm SHA256).Hash
    $env:PUBG_OCR_PROVIDER = 'cpu'
    $env:PATH = (Split-Path $ffmpeg) + ';' + $env:PATH
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $gui = Start-Process -FilePath $uiExecutable -WorkingDirectory $extractDir -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    do {
        if ($gui.HasExited) { throw "Final ZIP GUI exited before presenting a window (exit $($gui.ExitCode))" }
        $gui.Refresh()
        if ($gui.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($gui.MainWindowHandle -eq [IntPtr]::Zero) { throw "Final ZIP GUI has no accessible desktop window after 45s (session $($gui.SessionId), interactive=$([Environment]::UserInteractive)); run the mandatory GUI gate on an interactive Windows runner, not a service desktop" }
    $window = [Windows.Automation.AutomationElement]::FromHandle($gui.MainWindowHandle)
    Set-UiValue $window 'InputPathTextBox' $eventVideo
    Set-UiValue $window 'OutputPathTextBox' $output
    Set-UiValue $window 'BeforeTextBox' '1'
    Set-UiValue $window 'AfterTextBox' '1'
    Select-UiIndex $window 'LanguageComboBox' 2
    Select-UiIndex $window 'ScanModeComboBox' 2
    Set-UiToggle $window 'MergeCheckBox' $false
    # Expand expert settings by its existing accessible name, without modifying
    # WPF layout/automation properties solely for this release test.
    $expertName = -join @([char]0x4E13,[char]0x5BB6,[char]0x8BBE,[char]0x7F6E)
    $expertCondition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $expertName)
    $expert = $window.FindFirst([Windows.Automation.TreeScope]::Descendants, $expertCondition)
    if ($null -eq $expert) { throw 'GUI expert settings UIAutomation element is unavailable; cannot configure public OCR acceptance' }
    $expertExpanded = $false
    for ($level = 0; $level -lt 4 -and $null -ne $expert; $level++) {
        $toggle = $null
        if ($expert.TryGetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$toggle)) {
            $toggle.Expand(); $expertExpanded = $true; break
        }
        if ($expert.TryGetCurrentPattern([Windows.Automation.TogglePattern]::Pattern, [ref]$toggle)) {
            if ($toggle.Current.ToggleState -ne [Windows.Automation.ToggleState]::On) { $toggle.Toggle() }
            $expertExpanded = $true; break
        }
        $expert = [Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($expert)
    }
    if (-not $expertExpanded) { throw 'GUI expert settings exposes neither ExpandCollapse nor Toggle pattern' }
    Set-UiValue $window 'MinEventTextBox' '0'
    Set-UiValue $window 'JobsTextBox' '1'
    Set-UiToggle $window 'BrightnessGateCheckBox' $false
    $run = Find-UiControl $window 'RunButton'
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not $run.Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 250 }
    if (-not $run.Current.IsEnabled) { throw 'Final ZIP GUI did not enable Start after valid input/CLI initialization' }
    $run.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
    $deadline = [DateTime]::UtcNow.AddMinutes(3)
    $summaryPath = Join-Path $output 'summary.json'
    do {
        if ($gui.HasExited) { throw 'Final ZIP GUI exited during public OCR processing' }
        if ((Test-Path -LiteralPath $summaryPath) -and $run.Current.IsEnabled) { break }
        Start-Sleep -Milliseconds 500
    } while ([DateTime]::UtcNow -lt $deadline)
    if (-not (Test-Path -LiteralPath $summaryPath) -or -not $run.Current.IsEnabled) { throw 'Final ZIP GUI did not complete public OCR processing within 180s; inspect desktop/UIA session, no fallback success is allowed' }
    $summary = Get-Content -LiteralPath $summaryPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-PublicSmokeResult $summary $eventVideo $sourceHash $ffmpeg $ffprobe
    $included = Find-UiControl $window 'IncludedMetricText'
    if ($included.Current.Name -ne '1') { throw 'Final ZIP GUI did not display the completed included-count summary' }
    Write-Host 'Final ZIP self-contained GUI executable started directly and completed public OCR-to-trim; report/media/source checks passed (CI enforces outbound blocking). Nothing installed a runtime during this verification.'
}
finally {
    if ($null -ne $gui) {
        if (-not $gui.HasExited) { & taskkill /PID $gui.Id /T /F 2>$null | Out-Null }
        $gui.Dispose()
    }
    foreach ($rule in $firewallRules) { Remove-NetFirewallRule -Name $rule -ErrorAction SilentlyContinue }
    $env:PUBG_OCR_PROVIDER = $oldProvider
    $env:PATH = $oldPath
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
