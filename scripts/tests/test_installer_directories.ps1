# Real, non-elevated Windows filesystem tests of the public native helper only.
# This does NOT test CreateProtected or the elevated installer lifecycle.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\installer\DirectoryGuard.ps1')
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class PhtDirectoryFixture {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFile(string path, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool DeviceIoControl(SafeFileHandle file, uint code, byte[] input, int length, IntPtr output, int outputLength, out int returned, IntPtr overlapped);
    // Return the actual Win32 result, not a simulated lifecycle response.
    public static int SetJunction(string directory, string target) {
        using (var handle = CreateFile(directory, 0x40000000, 7, IntPtr.Zero, 3, 0x02200000, IntPtr.Zero)) {
            if (handle.IsInvalid) return Marshal.GetLastWin32Error();
            var substitute = Encoding.Unicode.GetBytes("\\??\\" + target);
            var print = Encoding.Unicode.GetBytes(target);
            var buffer = new byte[16 + substitute.Length + 2 + print.Length + 2];
            Array.Copy(BitConverter.GetBytes(0xA0000003u), 0, buffer, 0, 4);
            Array.Copy(BitConverter.GetBytes((ushort)(buffer.Length - 8)), 0, buffer, 4, 2);
            Array.Copy(BitConverter.GetBytes((ushort)substitute.Length), 0, buffer, 10, 2);
            Array.Copy(BitConverter.GetBytes((ushort)(substitute.Length + 2)), 0, buffer, 12, 2);
            Array.Copy(BitConverter.GetBytes((ushort)print.Length), 0, buffer, 14, 2);
            Array.Copy(substitute, 0, buffer, 16, substitute.Length);
            Array.Copy(print, 0, buffer, 18 + substitute.Length, print.Length);
            int returned;
            return DeviceIoControl(handle, 0x000900A4, buffer, buffer.Length, IntPtr.Zero, 0, out returned, IntPtr.Zero) ? 0 : Marshal.GetLastWin32Error();
        }
    }
}
'@
function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Assert-Rejected([scriptblock]$Action, [string]$Message) {
    try { & $Action } catch { return }
    throw $Message
}
$temp = Join-Path ([IO.Path]::GetTempPath()) ('pht-directories-' + [guid]::NewGuid().ToString('N'))
$chinese = -join ([char[]]@(0x4e2d, 0x6587))
$parent = Join-Path $temp ($chinese + ' spaces [parent] & special')
$directory = Join-Path $parent ($chinese + ' new [pin] & spaces')
$target = Join-Path $temp 'junction target'
$junction = Join-Path $parent 'junction baseline'
$pin = $null
try {
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    [IO.Directory]::CreateDirectory($target) | Out-Null
    [IO.Directory]::CreateDirectory($junction) | Out-Null
    $aclBefore = (Get-Acl -LiteralPath $parent).Sddl
    # Verify the real junction fixture works without elevation before testing denial.
    $result = [PhtDirectoryFixture]::SetJunction($junction, $target)
    Assert-True ($result -eq 0) "Unpinned junction fixture failed: Win32 $result"
    Assert-Rejected { $h = [PhtDirectoryGuard]::Pin($junction); $h.Dispose() } 'Pin accepted an existing junction.'
    [IO.Directory]::Delete($junction)

    $pin = [PhtDirectoryGuard]::Pin($directory)
    Assert-Rejected { [IO.Directory]::Move($directory, ($directory + '-renamed')) } 'Pinned directory was renamed.'
    $result = [PhtDirectoryFixture]::SetJunction($directory, $target)
    Assert-True ($result -eq 32) "Pinned directory permits junction mutation, or unexpected failure: Win32 $result (expected sharing violation 32)."
    # Excluding directory write sharing must not prevent ordinary child I/O.
    $child = Join-Path $directory ($chinese + ' [data].txt')
    [IO.File]::WriteAllText($child, 'real filesystem')
    Assert-True ([IO.File]::ReadAllText($child) -eq 'real filesystem') 'Child file I/O failed while pinned.'
    [IO.File]::Delete($child)
    $pin.Dispose(); $pin = $null

    # Identity assertions below deliberately require the public native Identity API.
    $pin = [PhtDirectoryGuard]::Pin($directory)
    $identity = [PhtDirectoryGuard]::Identity($pin)
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$identity)) 'Directory identity is empty.'
    $pin.Dispose(); $pin = $null
    $renamed = $directory + '-renamed'
    [IO.Directory]::Move($directory, $renamed)
    $pin = [PhtDirectoryGuard]::Pin($renamed)
    Assert-True ([PhtDirectoryGuard]::Identity($pin) -eq $identity) 'Identity changed after rename.'
    $pin.Dispose(); $pin = $null
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    $pin = [PhtDirectoryGuard]::Pin($directory)
    Assert-True ([PhtDirectoryGuard]::Identity($pin) -ne $identity) 'Reused path has the old directory identity.'
    $pin.Dispose(); $pin = $null
    $pin = [PhtDirectoryGuard]::Pin([IO.Path]::GetPathRoot($temp))
    $rootIdentity = [PhtDirectoryGuard]::Identity($pin)
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$rootIdentity)) 'Volume root identity is empty.'
    $pin.Dispose(); $pin = $null
    $pin = [PhtDirectoryGuard]::Pin([IO.Path]::GetPathRoot($temp))
    Assert-True ([PhtDirectoryGuard]::Identity($pin) -eq $rootIdentity) 'Volume root identity is not stable across handles.'
    $pin.Dispose(); $pin = $null
    Assert-True ((Get-Acl -LiteralPath $parent).Sddl -eq $aclBefore) 'User-owned parent ACL changed.'
    Write-Host 'Installer native directory tests passed (not elevated lifecycle coverage).'
} finally {
    if ($null -ne $pin) { $pin.Dispose() }
    # Never recurse through junctions, even if a failed assertion left one behind.
    foreach ($path in @($directory, $junction)) {
        if ([IO.Directory]::Exists($path) -and (([IO.File]::GetAttributes($path) -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            [IO.Directory]::Delete($path)
        }
    }
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}
