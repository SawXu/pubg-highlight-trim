# Public packaging boundary: only the Native artifact is accepted.
$ErrorActionPreference = 'Stop'
$entry = Join-Path $PSScriptRoot '..\build_installers_windows.ps1'
function Assert-Rejected([scriptblock]$Action, [string]$Message) {
    try { & $Action } catch {
        if ($_.Exception.Message -notlike "*$Message*") { throw }
        return
    }
    throw "Expected packaging rejection: $Message"
}
$command = Get-Command $entry -ErrorAction Stop
$variantAttribute = @($command.Parameters['Variant'].Attributes | Where-Object { $_ -is [Management.Automation.ValidateSetAttribute] })
if ($variantAttribute.Count -ne 1 -or @($variantAttribute.ValidValues) -cne @('Native')) { throw 'Packaging must expose only the Native artifact.' }
Assert-Rejected { & $entry -Variant Native -Version '1.2' -SkipCliBuild -SkipUiBuild } 'numeric semantic version'
Write-Host 'Native installer public input rejection tests passed.'
