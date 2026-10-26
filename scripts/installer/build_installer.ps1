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
$desktopProbe=Join-Path $PSScriptRoot 'DesktopRuntimeProbe.ps1'
# NSIS uses $ for escapes, not backslashes. Do not corrupt Windows paths.
function Escape-NsisString([string]$s){$s.Replace('$','$$').Replace('"','$\"')}
$template=@'
Unicode true
ManifestDPIAware true
SetFont "Microsoft YaHei UI" 9
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
RequestExecutionLevel admin
Name "PUBG Highlight Trim · __VARIANT_LABEL__"
OutFile "__OUT__"
InstallDir "$PROGRAMFILES64\PUBG Highlight Trim"
BrandingText "PUBG Highlight Trim"
ShowInstDetails nevershow
ShowUninstDetails nevershow

!define MUI_ICON "__ICON__"
!define MUI_UNICON "__ICON__"
!define MUI_ABORTWARNING
!define MUI_UNABORTWARNING
!define MUI_WELCOMEFINISHPAGE_BITMAP "__WIZARD__"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "__WIZARD__"
!define MUI_WELCOMEPAGE_TITLE "安装 PUBG Highlight Trim"
!define MUI_WELCOMEPAGE_TEXT "从精彩瞬间，到高光合集。$\r$\n$\r$\n__VARIANT_DESCRIPTION__$\r$\n$\r$\n下一步选择安装位置。程序及界面数据跟随该目录，升级保留设置。$\r$\n$\r$\n未签名版本可能显示安全提示。请核对来源，不要关闭安全软件。"
!insertmacro MUI_PAGE_WELCOME
!define MUI_DIRECTORYPAGE_TEXT_TOP "选择本地磁盘上的专用应用目录，支持中文和空格。两个版本不能并装；升级请保持原位置。注册信息和临时文件仍可能使用系统盘。"
!define MUI_DIRECTORYPAGE_TEXT_DESTINATION "安装位置（包含程序及 data 数据目录）"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TITLE "安装完成"
!define MUI_FINISHPAGE_TEXT "PUBG Highlight Trim 已准备就绪。$\r$\n$\r$\n关闭安装器后，从开始菜单正常启动应用。请勿以管理员身份运行。$\r$\n$\r$\n界面设置、日志及浏览器数据位于安装目录下的 data 文件夹。视频输出和 OCR 缓存遵循处理引擎的目录设置。"
!define MUI_FINISHPAGE_NOAUTOCLOSE
!insertmacro MUI_PAGE_FINISH
!define MUI_UNCONFIRMPAGE_TEXT_TOP "将删除应用及随包依赖。所有界面数据及非程序文件均会保留，不删除系统 WebView2。$\r$\n$\r$\n卸载后请人工检查 data 目录，仅删除你确认不再需要的文件。"
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!define MUI_FINISHPAGE_TITLE "卸载完成"
!define MUI_FINISHPAGE_TEXT "应用已卸载。$\r$\n$\r$\n界面数据和非程序文件已保留。请检查原安装目录，确认后再清理个人文件。"
!insertmacro MUI_UNPAGE_FINISH
!insertmacro MUI_LANGUAGE "SimpChinese"
Function .onInit
  IfSilent 0 +3
  MessageBox MB_OK|MB_ICONSTOP "不支持静默安装。请在安装页面中选择安装目录。"
  Abort
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
  File "__DESKTOP_PROBE__"
  WriteUninstaller "$PLUGINSDIR\payload\Uninstall.exe"
  StrCpy $2 "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
  ${If} ${RunningX64}
    StrCpy $2 "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
  ${EndIf}
  nsExec::ExecToLog '"$2" -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\install_transaction.ps1" -Operation Install -Layout "$PLUGINSDIR\payload" -Target "$INSTDIR"'
  Pop $1
  StrCmp $1 0 success
  SetDetailsView show
  MessageBox MB_OK|MB_ICONSTOP "安装失败或已取消。请展开详细信息查看原因；不会自动启动应用。"
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
  nsExec::ExecToLog '"$2" -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\install_transaction.ps1" -Operation Uninstall -Layout "$PLUGINSDIR" -Target "$INSTDIR"'
  Pop $1
  StrCmp $1 0 un_success
  SetDetailsView show
  MessageBox MB_OK|MB_ICONSTOP "卸载失败，数据已保留。请查看详细信息。"
  SetErrorLevel 1
  Abort
un_success:
  SetShellVarContext all
  Delete "$SMPROGRAMS\PUBG Highlight Trim\PUBG Highlight Trim.lnk"
  RMDir "$SMPROGRAMS\PUBG Highlight Trim"
SectionEnd
'@
$variantLabel='系统 WebView2 版';$variantDescription='使用本机已有的 WebView2（120+）。不会下载或补装运行时；如果环境不确定，请选择内置版。'
if($Variant -eq 'Fixed'){$variantLabel='内置 WebView2 版';$variantDescription='随包提供 WebView2，无需另装共享运行时。内置运行时缺失时不会回退系统版本。'}
$template.Replace('__VARIANT_LABEL__',$variantLabel).Replace('__VARIANT_DESCRIPTION__',$variantDescription).Replace('__ICON__',(Escape-NsisString (Join-Path $PSScriptRoot '../../ui/src-tauri/icons/icon.ico'))).Replace('__WIZARD__',(Escape-NsisString (Join-Path $PSScriptRoot 'wizard.bmp'))).Replace('__OUT__',(Escape-NsisString $exe)).Replace('__LAYOUT__',(Escape-NsisString $Layout)).Replace('__ENGINE__',(Escape-NsisString $engine)).Replace('__GUARD__',(Escape-NsisString $guard)).Replace('__DESKTOP_PROBE__',(Escape-NsisString $desktopProbe))|Set-Content -LiteralPath $nsis -Encoding UTF8
try{& $MakeNsis $nsis;if($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $exe)){throw 'NSIS build failed.'}}finally{Remove-Item -LiteralPath $nsis -Force -ErrorAction SilentlyContinue}
Write-Host "Built $exe"
