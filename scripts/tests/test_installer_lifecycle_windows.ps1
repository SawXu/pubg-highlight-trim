[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Setup,
 [Parameter(Mandatory)][string]$Target,
 [Parameter(Mandatory)][ValidateSet('system','fixed')][string]$Variant,
 [switch]$DisposableVm
)
$ErrorActionPreference='Stop'
# This test deliberately uses the shipping EXE, not a mocked transaction engine.
# No registry writes, ACL repair, runtime installation or automatic cleanup here.
if(!$DisposableVm){throw 'REFUSED: run only in a disposable Windows VM; explicitly pass -DisposableVm. Never run on the development host.'}
if([Environment]::OSVersion.Platform -ne 'Win32NT' -or ![Environment]::Is64BitOperatingSystem){throw 'Windows x64 is required.'}
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if(([Security.Principal.WindowsPrincipal]$identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Run from the intended standard-user Explorer desktop; supply separate administrator credentials only to UAC.'}
if((Read-Host 'Confirm this is a throwaway VM by typing DISPOSABLE VM') -cne 'DISPOSABLE VM'){throw 'Cancelled without launching anything.'}
$Setup=(Resolve-Path -LiteralPath $Setup).Path
if([IO.Path]::GetExtension($Setup) -ine '.exe'){throw 'Setup must be the real shipping installer EXE.'}
$Target=[IO.Path]::GetFullPath($Target).TrimEnd('\')
if($Target -notmatch '^[A-Za-z]:\\' -or $Target.Length -lt 4 -or (Test-Path -LiteralPath $Target)){throw 'Choose a new, non-root local installation directory.'}
$hive=[Microsoft.Win32.RegistryKey]::OpenBaseKey('LocalMachine','Registry64')
$registration='Software\Microsoft\Windows\CurrentVersion\Uninstall\pubg-highlight-trim'
function Read-Installation {
 $key=$hive.OpenSubKey($registration)
 if(!$key){return $null}
 try{return @{Location=$key.GetValue('InstallLocation');Variant=$key.GetValue('Variant')}}finally{$key.Dispose()}
}
function Get-DataSnapshot([string]$Root){
 $snapshot=@{}
 foreach($file in Get-ChildItem -LiteralPath $Root -File -Recurse -Force){
  if($file.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Unexpected test data reparse point.'}
  $snapshot[$file.FullName.Substring($Root.Length)]=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
 }
 return $snapshot
}
try {
 if(Read-Installation){throw 'Existing registered installation: use a clean VM snapshot. Nothing was launched.'}
 Write-Host 'Select exactly the target directory supplied to this script in setup (<install>; path not logged).'
 Write-Host 'Supply the separate administrator account to UAC. Do not start the UI yet.'
 $process=Start-Process -FilePath $Setup -PassThru -Wait
 if($process.ExitCode -ne 0){throw "Real setup returned $($process.ExitCode). Preserve details/screenshots; this run is not a pass."}
 [void](Read-Host 'After setup has fully completed, press Enter to inspect Windows state')
 $installed=Read-Installation
 if(!$installed -or $installed.Location -ine $Target -or $installed.Variant -ne $Variant){throw 'Real installer did not register the requested target/variant.'}
 foreach($name in @('pubg-highlight-trim-ui.exe','distribution.json','.installer\installer-manifest.json','Uninstall.exe')){
  if(!(Test-Path -LiteralPath (Join-Path $Target $name) -PathType Leaf)){throw "Missing installed file: $name"}
 }
 $data=Join-Path $Target 'data'
 if(!(Test-Path -LiteralPath $data -PathType Container)){throw 'Installer did not create UI data directory.'}
 # Test business files even when names resemble UI settings/log/cache output.
 # Standard-user writes also prove this target user can use the data directory.
 foreach($relative in @('settings.json','settings.tmp','run.log','business.txt','webview\Default\business.txt','webview\Cache\unknown.bin')){
  $file=Join-Path $data $relative
  if(Test-Path -LiteralPath $file){throw "Sentinel conflicts with preexisting data: $relative; do not overwrite it."}
  [void][IO.Directory]::CreateDirectory((Split-Path $file -Parent))
  [IO.File]::WriteAllBytes($file,[Text.Encoding]::UTF8.GetBytes("Business data sentinel: $relative`nNot disposable UI output."))
 }
 $before=Get-DataSnapshot $data
 Write-Host 'Uninstall must explain that ALL data is retained; no misleading automatic cleanup option.'
 $uninstaller=Start-Process -FilePath (Join-Path $Target 'Uninstall.exe') -PassThru -Wait
 [void](Read-Host 'NSIS may relaunch from a temporary path. After the FINAL uninstall window closes, press Enter')
 if(Read-Installation){throw 'Registration remains after real uninstall.'}
 if(Test-Path -LiteralPath (Join-Path $Target 'pubg-highlight-trim-ui.exe')){throw 'Owned UI executable remains after uninstall.'}
 $after=Get-DataSnapshot $data
 if($after.Count -ne $before.Count){throw 'Data file count changed during uninstall.'}
 foreach($relative in $before.Keys){if(!$after.ContainsKey($relative) -or $after[$relative] -ne $before[$relative]){throw "Business/UI data lost or changed: $relative"}}
 Write-Host 'PASS: real interactive installer/uninstaller registered and removed app; all data sentinels were byte-for-byte retained.'
 Write-Host 'Not covered by this run: rendering/offline runtime, upgrade/rollback, policy matrix, or automatic full cleanup (unsupported). Restore the VM snapshot; no test cleanup is attempted.'
} finally {$hive.Dispose()}
