param([string]$NativeSystemDll="$env:WINDIR\System32\kernel32.dll")
# Reproduce NSIS's plugin working directory without elevation or installation.
$ErrorActionPreference='Stop'
$root=Join-Path ([IO.Path]::GetTempPath()) ('pht-compiler-'+[guid]::NewGuid().ToString('N'))
$installer=(Resolve-Path (Join-Path $PSScriptRoot '..\installer')).Path
$previous=[Environment]::CurrentDirectory
New-Item -ItemType Directory -Path $root | Out-Null
try {
 Copy-Item -LiteralPath $NativeSystemDll -Destination (Join-Path $root 'System.dll')
 [Environment]::CurrentDirectory=$root
 . (Join-Path $installer 'DirectoryGuard.ps1')
 . (Join-Path $installer 'DesktopRuntimeProbe.ps1')
 if(-not ('PhtDirectoryGuard' -as [type]) -or -not ('PhtDesktopRuntimeProbe' -as [type])){throw 'Installer native helpers were not compiled.'}
 Write-Host 'Native installer helpers compile with a native System.dll in the working directory.'
} finally {
 [Environment]::CurrentDirectory=$previous
 Remove-Item -LiteralPath $root -Recurse -Force
}
