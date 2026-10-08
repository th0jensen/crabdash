using System;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;

public static class CrabdashProcessOwner
{
    [StructLayout(LayoutKind.Sequential)]
    private struct FileTime
    {
        public uint Low;
        public uint High;
        public long Ticks { get { return unchecked((long)(((ulong)High << 32) | Low)); } }
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessTimes(IntPtr process, out FileTime creation,
        out FileTime exit, out FileTime kernel, out FileTime user);
    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetTokenInformation(IntPtr token, int kind, IntPtr buffer,
        uint length, out uint returned);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetUserNameW(StringBuilder name, ref uint length);

    // CIM creation dates retain microseconds; native FILETIME retains 100ns.
    // Keep this comparison in integer C#: PowerShell division converts to double.
    public static bool MatchesCreationTime(long actual, long expected)
    {
        return actual > 0 && expected > 0 && actual / 10 == expected / 10;
    }

    private static string TokenSid(IntPtr token)
    {
        uint needed;
        GetTokenInformation(token, 1 /* TokenUser */, IntPtr.Zero, 0, out needed);
        if (needed < IntPtr.Size || needed > 65536) return null;
        IntPtr buffer = Marshal.AllocHGlobal((int)needed);
        try
        {
            uint returned;
            if (!GetTokenInformation(token, 1, buffer, needed, out returned)
                || returned < IntPtr.Size || returned > needed) return null;
            IntPtr sid = Marshal.ReadIntPtr(buffer);
            return sid == IntPtr.Zero ? null : new SecurityIdentifier(sid).Value;
        }
        finally { Marshal.FreeHGlobal(buffer); }
    }

    public static string SidForProcess(uint pid, long expectedCreationFileTime)
    {
        IntPtr process = IntPtr.Zero;
        IntPtr token = IntPtr.Zero;
        try
        {
            process = OpenProcess(0x1000 /* QUERY_LIMITED_INFORMATION */, false, pid);
            if (process == IntPtr.Zero) return null;
            FileTime creation, exit, kernel, user;
            if (!GetProcessTimes(process, out creation, out exit, out kernel, out user)
                || !MatchesCreationTime(creation.Ticks, expectedCreationFileTime)) return null;
            if (!OpenProcessToken(process, 8 /* TOKEN_QUERY */, out token)) return null;
            return TokenSid(token);
        }
        catch { return null; }
        finally
        {
            if (token != IntPtr.Zero) CloseHandle(token);
            if (process != IntPtr.Zero) CloseHandle(process);
        }
    }

    // Resolve only the already logged-on account once. No per-process SID name
    // lookup or domain translation is performed; other unknown owners keep SID.
    public static string[] CurrentAccount()
    {
        IntPtr token = IntPtr.Zero;
        try
        {
            if (!OpenProcessToken(GetCurrentProcess(), 8, out token)) return null;
            string sid = TokenSid(token);
            StringBuilder name = new StringBuilder(512);
            uint length = (uint)name.Capacity;
            if (sid == null || !GetUserNameW(name, ref length)) return null;
            return new string[] { sid, name.ToString() };
        }
        catch { return null; }
        finally { if (token != IntPtr.Zero) CloseHandle(token); }
    }
}
