# Run only the shipped, read-only --check-webview2-runtime contract. Never use
# the elevated administrator's HKCU or environment as the desktop user's probe.
# Resolve the .NET framework assembly, not NSIS's native System.dll plugin.
Add-Type -ReferencedAssemblies ([System.ComponentModel.Win32Exception].Assembly.Location) -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
public static class PhtDesktopRuntimeProbe {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct STARTUPINFO {
        public int cb; public string reserved, desktop, title;
        public int x, y, xSize, ySize, xChars, yChars, fill, flags;
        public short show, reservedSize; public IntPtr reservedPointer, input, output, error;
    }
    [StructLayout(LayoutKind.Sequential)] struct PROCESS_INFORMATION {
        public IntPtr process, thread; public uint processId, threadId;
    }
    [DllImport("user32.dll")] static extern IntPtr GetShellWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll", SetLastError=true)] static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError=true)] static extern bool DuplicateTokenEx(IntPtr token, uint access, IntPtr attributes, int level, int type, out IntPtr duplicate);
    [DllImport("advapi32.dll", SetLastError=true)] static extern bool GetTokenInformation(IntPtr token, int kind, out int value, int size, out int needed);
    [DllImport("userenv.dll", SetLastError=true)] static extern bool CreateEnvironmentBlock(out IntPtr environment, IntPtr token, bool inherit);
    [DllImport("userenv.dll")] static extern bool DestroyEnvironmentBlock(IntPtr environment);
    [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool CreateProcessWithTokenW(IntPtr token, uint logon, string application, StringBuilder command, uint flags, IntPtr environment, string directory, ref STARTUPINFO startup, out PROCESS_INFORMATION process);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetExitCodeProcess(IntPtr process, out uint code);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool TerminateProcess(IntPtr process, uint code);
    static void Check(bool ok) { if (!ok) throw new Win32Exception(Marshal.GetLastWin32Error()); }
    public static uint Run(string executable, string directory, string expectedSid, uint[] expectedExplorerPids, string variant) {
        if (variant!="system" && variant!="fixed") throw new ArgumentException("Invalid runtime variant.", "variant");
        IntPtr process=IntPtr.Zero, token=IntPtr.Zero, primary=IntPtr.Zero, environment=IntPtr.Zero;
        PROCESS_INFORMATION child=new PROCESS_INFORMATION();
        try {
            uint shellPid;
            IntPtr shellWindow=GetShellWindow();
            if (shellWindow==IntPtr.Zero || GetWindowThreadProcessId(shellWindow, out shellPid)==0 || Array.IndexOf(expectedExplorerPids, shellPid)<0)
                throw new InvalidOperationException("Cannot identify the intended Explorer shell; runtime preflight refused.");
            process=OpenProcess(0x1000, false, shellPid);
            Check(process!=IntPtr.Zero);
            Check(OpenProcessToken(process, 0x0002 | 0x0008, out token));
            using (var identity=new WindowsIdentity(token)) {
                if (identity.User.Value!=expectedSid) throw new InvalidOperationException("Desktop token identity mismatch.");
            }
            int elevated, needed;
            Check(GetTokenInformation(token, 20, out elevated, 4, out needed));
            if (elevated!=0) throw new InvalidOperationException("Desktop shell is elevated; start from a normal user desktop.");
            // A primary duplicate, not impersonation or ShellExecute: exact process
            // handle/exit code and the target user's HKCU and environment.
            Check(DuplicateTokenEx(token, 0x02000000, IntPtr.Zero, 2, 1, out primary));
            Check(CreateEnvironmentBlock(out environment, primary, false));
            var startup=new STARTUPINFO(); startup.cb=Marshal.SizeOf(typeof(STARTUPINFO));
            startup.desktop="winsta0\\default"; startup.flags=1; startup.show=0;
            var command=new StringBuilder("\""+executable+"\" --check-webview2-runtime "+variant);
            // No profile loading, console or inherited administrator environment.
            Check(CreateProcessWithTokenW(primary, 0, executable, command, 0x08000400, environment, directory, ref startup, out child));
            uint wait=WaitForSingleObject(child.process, 60000);
            if (wait!=0) {
                Check(TerminateProcess(child.process, 1));
                Check(WaitForSingleObject(child.process, 10000)==0);
                throw new InvalidOperationException("Desktop runtime probe timed out or could not be waited for.");
            }
            uint code; Check(GetExitCodeProcess(child.process, out code)); return code;
        } finally {
            if (child.thread!=IntPtr.Zero) CloseHandle(child.thread);
            if (child.process!=IntPtr.Zero) CloseHandle(child.process);
            if (environment!=IntPtr.Zero) DestroyEnvironmentBlock(environment);
            if (primary!=IntPtr.Zero) CloseHandle(primary);
            if (token!=IntPtr.Zero) CloseHandle(token);
            if (process!=IntPtr.Zero) CloseHandle(process);
        }
    }
}
'@
