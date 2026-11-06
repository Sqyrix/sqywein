using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Sqywein;

/// <summary>Görev Yöneticisi'nin "Bellek" sütunuyla aynı ölçü: özel çalışma kümesi.</summary>
public static class MemoryMeter
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Counters
    {
        public uint cb;
        public uint PageFaultCount;
        public nuint PeakWorkingSetSize, WorkingSetSize,
            QuotaPeakPagedPoolUsage, QuotaPagedPoolUsage,
            QuotaPeakNonPagedPoolUsage, QuotaNonPagedPoolUsage,
            PagefileUsage, PeakPagefileUsage, PrivateUsage,
            PrivateWorkingSetSize, SharedCommitUsage;
    }

    [DllImport("kernel32.dll", EntryPoint = "K32GetProcessMemoryInfo", SetLastError = true)]
    private static extern bool GetInfo(IntPtr process, ref Counters counters, uint size);

    public static long PrivateBytes(int pid)
    {
        try
        {
            using var p = Process.GetProcessById(pid);
            var c = new Counters { cb = (uint)Marshal.SizeOf<Counters>() };
            return GetInfo(p.Handle, ref c, c.cb) ? (long)c.PrivateWorkingSetSize : p.WorkingSet64;
        }
        catch (Exception)
        {
            return 0; // Süreç bu arada kapanmış olabilir.
        }
    }

    public static long Total(IEnumerable<int> pids)
    {
        long sum = 0;
        foreach (var pid in pids) sum += PrivateBytes(pid);
        return sum;
    }
}
