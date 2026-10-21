# Windows-only directory pinning for elevated installer transactions.
# Handles exclude FILE_SHARE_DELETE, preventing rename/replacement while the
# engine inspects and operates on a directory. Reparse points are never followed.
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class PhtDirectoryGuard {
    [StructLayout(LayoutKind.Sequential)] struct SECURITY_ATTRIBUTES {
        public int length; public IntPtr descriptor; public int inherit;
    }
    [StructLayout(LayoutKind.Sequential)] struct FILE_INFO {
        public uint attributes; public System.Runtime.InteropServices.ComTypes.FILETIME creation;
        public System.Runtime.InteropServices.ComTypes.FILETIME access;
        public System.Runtime.InteropServices.ComTypes.FILETIME write;
        public uint volume, sizeHigh, sizeLow, links, indexHigh, indexLow;
    }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFile(string path, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandle(SafeFileHandle handle, out FILE_INFO info);
    [StructLayout(LayoutKind.Sequential)] struct FILE_ID_INFO { public ulong volume; public ulong idLow; public ulong idHigh; }
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandleEx(SafeFileHandle handle, int infoClass, out FILE_ID_INFO info, uint size);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool CreateDirectory(string path, ref SECURITY_ATTRIBUTES attributes);
    [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool ConvertStringSecurityDescriptorToSecurityDescriptor(string sddl, uint revision, out IntPtr descriptor, out uint size);
    [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr value);
    public static SafeFileHandle Pin(string path) {
        var handle = CreateFile(path, 0x80000000, 1, IntPtr.Zero, 3, 0x02200000, IntPtr.Zero);
        if (handle.IsInvalid) { handle.Dispose(); throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot pin directory: " + path); }
        FILE_INFO info;
        if (!GetFileInformationByHandle(handle, out info)) { var error=Marshal.GetLastWin32Error(); handle.Dispose(); throw new Win32Exception(error); }
        if ((info.attributes & 0x400) != 0 || (info.attributes & 0x10) == 0) { handle.Dispose(); throw new IOException("Not a plain directory: " + path); }
        return handle;
    }
    public static string Identity(SafeFileHandle handle) {
        FILE_ID_INFO info;
        if (GetFileInformationByHandleEx(handle, 18, out info, (uint)Marshal.SizeOf(typeof(FILE_ID_INFO))))
            return info.volume.ToString("X16") + ":" + info.idHigh.ToString("X16") + info.idLow.ToString("X16");
        FILE_INFO legacy;
        if (!GetFileInformationByHandle(handle, out legacy)) throw new Win32Exception(Marshal.GetLastWin32Error());
        return legacy.volume.ToString("X8") + ":" + legacy.indexHigh.ToString("X8") + legacy.indexLow.ToString("X8");
    }
    public static void CreateProtected(string path) {
        IntPtr descriptor; uint size;
        // New workspaces are administrator-owned from their first instant.
        if (!ConvertStringSecurityDescriptorToSecurityDescriptor("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)", 1, out descriptor, out size))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            var security = new SECURITY_ATTRIBUTES { length=Marshal.SizeOf(typeof(SECURITY_ATTRIBUTES)), descriptor=descriptor, inherit=0 };
            if (!CreateDirectory(path, ref security)) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot create private workspace: " + path);
        } finally { LocalFree(descriptor); }
    }
}
'@

function Add-InstallerDirectoryPins {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)]$Handles)
    $ancestors = New-Object 'Collections.Generic.List[string]'
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        $ancestors.Add($cursor)
        $parent = [IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
    for ($i = $ancestors.Count - 1; $i -ge 0; $i--) {
        if (Test-Path -LiteralPath $ancestors[$i] -PathType Container) {
            $Handles.Add([PhtDirectoryGuard]::Pin($ancestors[$i]))
        }
    }
}
