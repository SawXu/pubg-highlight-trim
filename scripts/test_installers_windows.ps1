[CmdletBinding()]
param(
    [ValidateSet('System','Fixed')][string]$Variant = 'System',
    [string]$InstallerPath,
    [ValidateSet('audit','fresh','running','upgrade','switch','locked','retry','uninstall')][string]$Phase = 'audit',
    [string]$InstallRoot,
    [string]$EvidenceDirectory = (Join-Path $PWD 'artifacts\windows-installer-verification'),
    [switch]$RunInteractive,
    [switch]$DisposableVM
)
$ErrorActionPreference = 'Stop'
if ($RunInteractive -and -not $DisposableVM) { throw 'Interactive execution requires explicit -DisposableVM authorization.' }
if ([IntPtr]::Size -ne 8) { throw 'Use 64-bit Windows PowerShell/pwsh.' }
if (-not $InstallRoot) { $InstallRoot = Join-Path $env:ProgramFiles 'PUBG Highlight Trim' }
$InstallRoot = [IO.Path]::GetFullPath($InstallRoot).TrimEnd('\')
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$isElevated = ([Security.Principal.WindowsPrincipal]$identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($RunInteractive -and $isElevated) { throw 'Use the ordinary desktop shell. Let the public EXE request UAC.' }
$evidence = Join-Path $EvidenceDirectory ((Get-Date -Format 'yyyyMMdd-HHmmss-fff') + "-$($Variant.ToLowerInvariant())-$Phase")
New-Item -ItemType Directory -Path $evidence -Force | Out-Null
function Save-Json([string]$Name, $Value) {
    $Value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidence $Name) -Encoding UTF8
}
function In-Install([string]$Path) {
    return ($Path -and ($Path -ieq $InstallRoot -or $Path.StartsWith($InstallRoot+'\', [StringComparison]::OrdinalIgnoreCase)))
}
function Safe-Path([string]$Path) {
    if (!$Path) { return '<unavailable>' }
    # Never emit unknown/possibly user-created names, even beneath the install root.
    if (In-Install $Path) {
        if ($Path -ieq $InstallRoot) { return '<install>' }
        $relative = $Path.Substring($InstallRoot.Length+1)
        if ($relative -in @('data','WebView2Runtime','distribution.json','installer-manifest.json','Uninstall.exe','pubg-highlight-trim-ui.exe','vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','msvcp140_1.dll','data\settings.json','data\settings.tmp','data\run.log')) { return '<install>\'+$relative }
        if ($relative -match '^(WebView2Runtime|cli)\\') { return '<install>\'+$matches[1]+'\<redacted>' }
        return '<install>\<redacted>'
    }
    foreach ($root in @($env:windir, $env:ProgramFiles, ${env:ProgramFiles(x86)})) {
        if ($root -and $Path.StartsWith($root+'\', [StringComparison]::OrdinalIgnoreCase)) { return '<system-runtime>' }
    }
    return '<redacted>'
}
function Role([string]$Sid) {
    if ($Sid -eq 'S-1-5-18') { return 'SYSTEM' }
    if ($Sid -eq 'S-1-5-32-544') { return 'Administrators' }
    if ($Sid -eq $identity.User.Value) { return 'current-desktop-user' }
    if ($Sid -in @('S-1-15-2-1','S-1-15-2-2')) { return 'AppContainer-packages' }
    return 'other-principal'
}
function Capture-Audit {
    $reg = Get-ItemProperty 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\pubg-highlight-trim' -ErrorAction SilentlyContinue
    Save-Json 'registry.json' ([ordered]@{ present=($null -ne $reg); installLocation=(Safe-Path $reg.InstallLocation); locationMatches=($reg.InstallLocation -ieq $InstallRoot); variant=(@('system','fixed') | Where-Object { $_ -eq $reg.Variant }); uninstallPointsToInstall=($reg.UninstallString -eq ('"'+(Join-Path $InstallRoot 'Uninstall.exe')+'"')) })
    $files = @()
    if (Test-Path -LiteralPath $InstallRoot) {
        $files = @(Get-ChildItem -LiteralPath $InstallRoot -Recurse -Force -File -ErrorAction SilentlyContinue | ForEach-Object {
            $hash=$null; try { $hash=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash } catch {}
            [ordered]@{ path=(Safe-Path $_.FullName); length=$_.Length; sha256=$hash }
        })
    }
    Save-Json 'files.json' $files
    # Raw CIM objects/identifiers/command lines stay in memory only.
    $raw = @(Get-CimInstance Win32_Process -Filter "Name='pubg-highlight-trim-ui.exe' OR Name='pubg-highlight-trim.exe' OR Name='msedgewebview2.exe'" -ErrorAction SilentlyContinue)
    $appIds = @($raw | Where-Object { $_.Name -ne 'msedgewebview2.exe' -and (In-Install $_.ExecutablePath) } | ForEach-Object { $_.ProcessId })
    $ownedIds = @($appIds)
    for ($i=0; $i -lt $raw.Count; $i++) {
        $ownedIds += @($raw | Where-Object { $_.ParentProcessId -in $ownedIds } | ForEach-Object { $_.ProcessId })
        $ownedIds = @($ownedIds | Select-Object -Unique)
    }
    $processes = @(); $modules = @()
    foreach ($proc in $raw) {
        $processes += [ordered]@{ name=$proc.Name; path=(Safe-Path $proc.ExecutablePath); inInstallRoot=[bool](In-Install $proc.ExecutablePath); associatedWithInstalledApp=($proc.ProcessId -in $ownedIds) }
        if ($proc.ProcessId -notin $ownedIds) { continue }
        try {
            foreach ($module in (Get-Process -Id $proc.ProcessId -ErrorAction Stop).Modules) {
                $modules += [ordered]@{ process=$proc.Name; path=(Safe-Path $module.FileName); inInstallRoot=[bool](In-Install $module.FileName) }
            }
        } catch { $modules += [ordered]@{ process=$proc.Name; error='MODULE_ACCESS_UNAVAILABLE' } }
    }
    Save-Json 'processes.json' $processes
    Save-Json 'loaded-modules.json' $modules
    Save-Json 'network-adapters.json' @(Get-NetAdapter -ErrorAction SilentlyContinue | ForEach-Object { [ordered]@{ up=($_.Status -eq 'Up') } })
    $shared = @(foreach ($key in @('HKLM:\Software\Microsoft\EdgeUpdate\Clients','HKLM:\Software\WOW6432Node\Microsoft\EdgeUpdate\Clients','HKCU:\Software\Microsoft\EdgeUpdate\Clients')) {
        foreach ($child in @(Get-ChildItem -LiteralPath $key -ErrorAction SilentlyContinue)) {
            $entry=Get-ItemProperty -LiteralPath $child.PSPath -ErrorAction SilentlyContinue
            if ($entry.name -match 'WebView2') { [ordered]@{ webviewRegistrationPresent=$true } }
        }
    })
    Save-Json 'shared-webview-registration.json' $shared
    $acl = @()
    foreach ($p in @($InstallRoot, (Join-Path $InstallRoot 'data'), (Join-Path $InstallRoot 'WebView2Runtime'))) {
        if (Test-Path -LiteralPath $p) {
            try {
                $a=Get-Acl -LiteralPath $p
                $rules=@($a.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]) | ForEach-Object { [ordered]@{ role=(Role $_.IdentityReference.Value); rights=$_.FileSystemRights.ToString(); type=$_.AccessControlType.ToString(); inherited=$_.IsInherited } })
                $acl += [ordered]@{ path=(Safe-Path $p); ownerRole=(Role $a.GetOwner([Security.Principal.SecurityIdentifier]).Value); protected=$a.AreAccessRulesProtected; rules=$rules }
            } catch { $acl += [ordered]@{ path=(Safe-Path $p); error='ACL_UNAVAILABLE' } }
        }
    }
    Save-Json 'paths-and-acl.json' $acl
    # Never copy settings or arbitrary log text. Extract only fixed known runtime fields.
    $logPath=Join-Path $InstallRoot 'data\run.log'
    $log=@()
    if (Test-Path -LiteralPath $logPath -PathType Leaf) {
        try { $log=@(Get-Content -LiteralPath $logPath | Where-Object { $_ -match 'RUNTIME' } | ForEach-Object { [ordered]@{ runtimeEvent=$true; mentionsFixed=($_ -match '(?i)fixed'); mentionsSystem=($_ -match '(?i)system'); mentionsInstallRoot=($_.IndexOf($InstallRoot,[StringComparison]::OrdinalIgnoreCase) -ge 0) } }) } catch {}
    }
    Save-Json 'runtime-log-sanitized.json' $log
    Save-Json 'environment.json' ([ordered]@{ osBuild=(Get-CimInstance Win32_OperatingSystem).BuildNumber; powershell=$PSVersionTable.PSVersion.ToString(); sharedRuntimeAbsenceProven=$false; offlineProven=$false })
    Save-Json 'summary.json' ([ordered]@{ phase=$Phase; requestedVariant=$Variant; elevated=$isElevated; fileCount=$files.Count; verdict='NOT EVALUATED - manual review required'; privacy='No raw identifiers, command lines, settings or private paths are persisted' })
}
Write-Host 'Writing sanitized evidence under the selected evidence directory (private path not printed).'
if ($RunInteractive) {
    if (-not $InstallerPath -or -not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) { throw 'A real public installer/uninstaller EXE is required.' }
    $resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
    if ([IO.Path]::GetExtension($resolvedInstaller) -ine '.exe') { throw 'Only public EXE entry points are allowed.' }
    Capture-Audit
    $before=Join-Path $evidence 'before'; New-Item -ItemType Directory -Path $before | Out-Null
    Get-ChildItem -LiteralPath $evidence -File | Move-Item -Destination $before
    $installerHash=(Get-FileHash -LiteralPath $resolvedInstaller -Algorithm SHA256).Hash
    if ((Read-Host 'Confirm disposable VM and real interactive phase by typing RUN') -cne 'RUN') { throw 'Cancelled before launch.' }
    $null=Start-Process -FilePath $resolvedInstaller -PassThru
    Save-Json 'installer-process.json' ([ordered]@{ sha256=$installerHash; variant=$Variant; launched=$true })
    $null=Read-Host 'Finish/close all installer UI, record outcome manually, then press Enter. Launch does not imply acceptance'
}
Capture-Audit
Write-Host 'Sanitized audit complete. No lifecycle acceptance verdict was inferred.'
