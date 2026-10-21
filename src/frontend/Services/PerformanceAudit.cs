using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Cleanup.Services;

/// <summary>Explicit local audit only: no timer, telemetry, or work in ordinary launches.</summary>
internal static class PerformanceAudit
{
    private static readonly bool Enabled = Environment.GetCommandLineArgs().Contains("--performance-audit", StringComparer.Ordinal);
    private static readonly SemaphoreSlim Gate = new(1);
    private static readonly AuditSnapshot Snapshot = new() { ProcessId = Environment.ProcessId };

    public static void WindowLoaded()
    {
        if (!Enabled) return;
        using var process = Process.GetCurrentProcess();
        Snapshot.WindowLoadedMs = (DateTime.UtcNow - process.StartTime.ToUniversalTime()).TotalMilliseconds;
        Capture(process);
        _ = WriteAsync();
    }

    public static void InventoryReady(int count, double milliseconds)
    {
        if (!Enabled) return;
        Snapshot.InventoryCount = count;
        Snapshot.InventoryMs = milliseconds;
        using var process = Process.GetCurrentProcess();
        Capture(process);
        _ = WriteAsync();
    }

    // The managed heap is separated from working set so a change can be attributed to .NET objects or to native XAML/runtime memory.
    private static void Capture(Process process)
    {
        GCMemoryInfo gc = GC.GetGCMemoryInfo();
        Snapshot.ManagedHeapMiB = Mib(GC.GetTotalMemory(forceFullCollection: false));
        Snapshot.GcCommittedMiB = Mib(gc.TotalCommittedBytes);
        Snapshot.WorkingSetMiB = Mib(process.WorkingSet64);
        Snapshot.PrivateMiB = Mib(process.PrivateMemorySize64);
        Snapshot.Handles = process.HandleCount;
        Snapshot.Threads = process.Threads.Count;
        Snapshot.Gen2Collections = GC.CollectionCount(2);
    }

    private static double Mib(long bytes) => Math.Round(bytes / 1048576.0, 2);

    private static async Task WriteAsync()
    {
        await Gate.WaitAsync();
        try
        {
            string path = Path.Combine(AppEnvironment.LocalStateDirectory, "performance-audit.json");
            await File.WriteAllTextAsync(path + ".tmp", JsonSerializer.Serialize(Snapshot, AuditJsonContext.Default.AuditSnapshot));
            File.Move(path + ".tmp", path, overwrite: true);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException) { /* An audit never blocks the app. */ }
        finally { Gate.Release(); }
    }
}

internal sealed class AuditSnapshot
{
    public int ProcessId { get; set; }
    public double WindowLoadedMs { get; set; }
    public int InventoryCount { get; set; }
    public double InventoryMs { get; set; }
    public double ManagedHeapMiB { get; set; }
    public double GcCommittedMiB { get; set; }
    public double WorkingSetMiB { get; set; }
    public double PrivateMiB { get; set; }
    public int Handles { get; set; }
    public int Threads { get; set; }
    public int Gen2Collections { get; set; }
}

[JsonSerializable(typeof(AuditSnapshot))]
internal sealed partial class AuditJsonContext : JsonSerializerContext;
