[CmdletBinding()]
param([Parameter(Mandatory)][string]$Layout,[Parameter(Mandatory)][string]$OutputDirectory,[Parameter(Mandatory)][ValidateSet('System','Fixed')][string]$Variant,[string]$MakeNsis='makensis')
$ErrorActionPreference='Stop'
$Layout=(Resolve-Path -LiteralPath $Layout).Path
if($Layout.StartsWith('\\')){throw 'Layout must be a local path.'}
$d=Get-Content -LiteralPath (Join-Path $Layout 'distribution.json') -Raw|ConvertFrom-Json
if($d.schema -ne 'pubg-highlight-trim/distribution/v1' -or $d.variant -ne $Variant.ToLowerInvariant()){throw 'distribution.json does not match requested variant.'}
$m=Get-Content -LiteralPath (Join-Path $Layout 'installer-manifest.json') -Raw|ConvertFrom-Json
if($m.schema -ne 'pubg-highlight-trim/installer-ownership/v1' -or $m.variant -ne $Variant.ToLowerInvariant()){throw 'installer manifest does not match requested variant.'}
if(!(Get-Command $MakeNsis -ErrorAction SilentlyContinue)){throw 'makensis is required to build the installer.'}
$compilerVersion=(& $MakeNsis /VERSION | Out-String).Trim()
if($compilerVersion -notmatch '^v(\d+\.\d+)' -or [version]$Matches[1] -lt [version]'3.11'){throw 'NSIS 3.11 or later is required for secure elevated temporary extraction.'}
$out=[IO.Path]::GetFullPath($OutputDirectory);New-Item -ItemType Directory -Path $out -Force|Out-Null
$nsis=Join-Path $out ('.pht-'+[guid]::NewGuid().ToString('N')+'.nsi');$exe=Join-Path $out ('pubg-highlight-trim-'+$Variant.ToLowerInvariant()+'-setup.exe')
$engine=Join-Path $PSScriptRoot 'install_transaction.ps1'
$guard=Join-Path $PSScriptRoot 'DirectoryGuard.ps1'
# NSIS uses $ for escapes, not backslashes. Do not corrupt Windows paths.
function Escape-NsisString([string]$s){$s.Replace('$','$$').Replace('"','$\"')}
$template=@'
Unicode true
!include "LogicLib.nsh"
!include "x64.nsh"
RequestExecutionLevel admin
Name "PUBG Highlight Trim (__VARIANT__)"
OutFile "__OUT__"
InstallDir "$PROGRAMFILES64\PUBG Highlight Trim"
ShowInstDetails show
ShowUninstDetails show
Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles
Function .onInit
  IfSilent 0 +3
  MessageBox MB_OK|MB_ICONSTOP "Silent installation is not supported. Choose an installation directory interactively."
  Abort
  MessageBox MB_OK "Choose a local installation directory. Both variants share one installation; upgrades keep that directory. Program, CLI and UI data follow it. Registry and temporary files may use the system disk. Unsigned builds may show Unknown Publisher/SmartScreen; do not disable security software. Launch the app normally from its shortcut, never as administrator."
  SetRegView 64
  ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\pubg-highlight-trim" "InstallLocation"
  StrCmp $0 "" +2
  StrCpy $INSTDIR $0
FunctionEnd
Section "Install"
  InitPluginsDir
  SetOutPath "$PLUGINSDIR\payload"
  File /r "__LAYOUT__\*"
  SetOutPath "$PLUGINSDIR"
  File "__ENGINE__"
  File "__GUARD__"
  WriteUninstaller "$PLUGINSDIR\payload\Uninstall.exe"
  StrCpy $2 "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
  ${If} ${RunningX64}
    StrCpy $2 "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
  ${EndIf}
  nsExec::ExecToLog '"$2" -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\install_transaction.ps1" -Operation Install -Layout "$PLUGINSDIR\payload" -Target "$INSTDIR"'
  Pop $1
  StrCmp $1 0 success
  MessageBox MB_OK|MB_ICONSTOP "Installation failed or was cancelled. Inspect the details above. No application will be launched."
  SetErrorLevel 1
  Abort
success:
  SetShellVarContext all
  CreateDirectory "$SMPROGRAMS\PUBG Highlight Trim"
  CreateShortcut "$SMPROGRAMS\PUBG Highlight Trim\PUBG Highlight Trim.lnk" "$INSTDIR\pubg-highlight-trim-ui.exe" "" "$INSTDIR\pubg-highlight-trim-ui.exe" 0 SW_SHOWNORMAL
SectionEnd
Section "Uninstall"
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File "__ENGINE__"
  File "__GUARD__"
  StrCpy $2 "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
  ${If} ${RunningX64}
    StrCpy $2 "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
  ${EndIf}
  StrCpy $3 ""
  MessageBox MB_YESNO|MB_DEFBUTTON2 "Also delete UI settings.json/settings.tmp/run.log? Default No preserves all data. Browser/cache and unknown files are always retained for manual inspection." IDNO keep_data
  StrCpy $3 "-DeleteData"
keep_data:
  nsExec::ExecToLog '"$2" -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\install_transaction.ps1" -Operation Uninstall -Layout "$PLUGINSDIR" -Target "$INSTDIR" $3'
  Pop $1
  StrCmp $1 0 un_success
  MessageBox MB_OK|MB_ICONSTOP "Uninstall failed. Data is retained. Inspect details above."
  SetErrorLevel 1
  Abort
un_success:
  SetShellVarContext all
  Delete "$SMPROGRAMS\PUBG Highlight Trim\PUBG Highlight Trim.lnk"
  RMDir "$SMPROGRAMS\PUBG Highlight Trim"
  MessageBox MB_OK "Uninstall completed. UI data and all unowned files were retained. Inspect the data directory manually before deleting any personal data."
SectionEnd
'@
$template.Replace('__VARIANT__',$Variant).Replace('__OUT__',(Escape-NsisString $exe)).Replace('__LAYOUT__',(Escape-NsisString $Layout)).Replace('__ENGINE__',(Escape-NsisString $engine)).Replace('__GUARD__',(Escape-NsisString $guard))|Set-Content -LiteralPath $nsis -Encoding UTF8
try{& $MakeNsis $nsis;if($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $exe)){throw 'NSIS build failed.'}}finally{Remove-Item -LiteralPath $nsis -Force -ErrorAction SilentlyContinue}
Write-Host "Built $exe"
