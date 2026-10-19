param(
    [Parameter(Mandatory = $true)][string]$UiExecutable,
    [Parameter(Mandatory = $true)][string]$CliExecutable
)
# Requires an interactive Windows desktop and a real CLI bundle with pinned models/FFmpeg.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('pubg-ui-unreadable-' + [guid]::NewGuid())
$oldCli = $env:PUBG_HIGHLIGHT_TRIM_CLI
$oldProvider = $env:PUBG_OCR_PROVIDER
$gui = $null

function Control([string]$Id) {
    $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)
    $element = $window.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $element) { throw "GUI control unavailable: $Id" }
    return $element
}
function Set-Value([string]$Id, [string]$Value) {
    (Control $Id).GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
}
try {
    $ui = (Resolve-Path -LiteralPath $UiExecutable).Path
    $env:PUBG_HIGHLIGHT_TRIM_CLI = (Resolve-Path -LiteralPath $CliExecutable).Path
    $env:PUBG_OCR_PROVIDER = 'cpu'
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    $source = Join-Path $tempRoot 'broken.mp4'
    $output = Join-Path $tempRoot 'output'
    [IO.File]::WriteAllText($source, 'synthetic invalid media container')
    $sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    $gui = Start-Process -FilePath $ui -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        if ($gui.HasExited) { throw 'GUI exited before showing a window' }
        $gui.Refresh()
        if ($gui.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($gui.MainWindowHandle -eq [IntPtr]::Zero) { throw 'No interactive GUI window; test cannot be skipped' }
    $window = [Windows.Automation.AutomationElement]::FromHandle($gui.MainWindowHandle)
    Set-Value 'InputPathTextBox' $source
    Set-Value 'OutputPathTextBox' $output
    $run = Control 'RunButton'
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not $run.Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
    if (-not $run.Current.IsEnabled) { throw 'GUI did not enable Start' }
    $run.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
    $deadline = [DateTime]::UtcNow.AddSeconds(90)
    do {
        if ($gui.HasExited) { throw 'GUI exited during processing' }
        $title = (Control 'RunTitleText').Current.Name
        if ($run.Current.IsEnabled -and $title -in @('视频读取失败', '处理失败', '扫描完成，没有匹配事件')) { break }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($title -ne '视频读取失败') { throw 'GUI did not report video read failure' }
    if ((Control 'ActivityTitleText').Current.Name -ne '视频读取失败') { throw 'Activity panel contradicts read failure' }
    if ((Control 'FooterStatusText').Current.Name -ne '读取失败，请检查运行记录') { throw 'Footer contradicts read failure' }
    if (-not (Control 'OpenOutputButton').Current.IsEnabled) { throw 'Output reports are not accessible' }
    if ((Control 'RunSubtitleText').Current.Name -notmatch '读取失败 1 / 1') { throw 'Read failure counts are missing' }
    $summary = Get-Content -LiteralPath (Join-Path $output 'summary.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($summary.source_count -ne 1 -or $summary.unreadable_count -ne 1 -or $summary.included_count -ne 0) { throw 'Unexpected actual CLI summary' }
    $rows = @(Import-Csv -LiteralPath $summary.csv -Encoding UTF8)
    if ($rows.Count -ne 1 -or $rows[0].Status -ne 'unreadable' -or $rows[0].Method -ne 'video-unreadable') { throw 'Unreadable CSV record was not preserved' }
    (Control 'LogToggleButton').GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
    $logs = (Control 'LogListBox').FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    $logText = ($logs | ForEach-Object { $_.Current.Name }) -join "`n"
    if ($logText -notmatch 'EXIT   code=2' -or $logText -notmatch 'SUMMARY ') { throw 'GUI did not retain the CLI summary and exit=2 log' }
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $sourceHash) { throw 'Source file was modified' }
    Write-Host 'PASS: actual bad MP4 -> CLI exit=2 -> WPF read failure; counts, activity, footer, logs, CSV and source integrity verified.'
}
finally {
    if ($null -ne $gui) {
        if (-not $gui.HasExited) { & taskkill /PID $gui.Id /T /F 2>$null | Out-Null }
        $gui.Dispose()
    }
    $env:PUBG_HIGHLIGHT_TRIM_CLI = $oldCli
    $env:PUBG_OCR_PROVIDER = $oldProvider
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
