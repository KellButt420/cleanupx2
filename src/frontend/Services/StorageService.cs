using System.Runtime.InteropServices;
using Cleanup.ViewModels;

namespace Cleanup.Services;

/// <summary>Reads the drives this user can see when asked. It never watches for changes.</summary>
internal static partial class StorageService
{
    // A disconnected network share can stall a size query for a long time, so each drive is
    // probed on its own thread and the page stops waiting after a limit. An optical drive can
    // take several seconds to spin up after a disc is inserted, so it gets a longer limit.
    private static readonly TimeSpan NetworkProbeTimeout = TimeSpan.FromSeconds(4);
    private static readonly TimeSpan LocalProbeTimeout = TimeSpan.FromSeconds(15);
    private static readonly SingleFlight<string, DriveItemViewModel> Probes = new(32);
    private static readonly SingleFlight<string, DriveInfo[]> DriveLists = new(1);
    private static readonly System.Collections.Concurrent.ConcurrentDictionary<string, DriveType> Types = new();

    public static async Task<DriveItemViewModel[]> ReadAsync()
    {
        // The type is a quick lookup, so it is read here with the drive list and kept even if probing times out.
        DriveInfo[] drives = await DriveLists.Run("drives", DriveInfo.GetDrives).WaitAsync(LocalProbeTimeout);
        return await Task.WhenAll(drives.Select(ProbeAsync));
    }

    private static async Task<DriveItemViewModel> ProbeAsync(DriveInfo drive)
    {
        string letter = drive.Name.TrimEnd('\\');
        DriveType type = Types.GetValueOrDefault(letter, DriveType.Unknown);
        TimeSpan timeout = type == DriveType.Network ? NetworkProbeTimeout : LocalProbeTimeout;
        try
        {
            return await Probes.Run(letter, () => {
                DriveType actual = drive.DriveType;
                Types[letter] = actual;
                return Probe(drive, actual);
            }).WaitAsync(timeout);
        }
        catch (TimeoutException)
        {
            return new DriveItemViewModel(letter, KindOf(type), "", "", null, Ready: false, Unresponsive: true, Total: 0, Free: 0);
        }
    }

    private static DriveKind KindOf(DriveType type) => type switch
    {
        DriveType.Fixed => DriveKind.Fixed,
        DriveType.Removable => DriveKind.Removable,
        DriveType.Network => DriveKind.Network,
        DriveType.CDRom => DriveKind.Optical,
        DriveType.Ram => DriveKind.Ram,
        _ => DriveKind.Other,
    };

    private static DriveItemViewModel Probe(DriveInfo drive, DriveType type)
    {
        string letter = drive.Name.TrimEnd('\\');
        DriveKind kind = KindOf(type);
        string? remote = kind == DriveKind.Network ? RemotePath(letter) : null;
        try
        {
            if (!drive.IsReady)
            {
                return new DriveItemViewModel(letter, kind, "", "", remote, Ready: false, Unresponsive: false, Total: 0, Free: 0);
            }
            return new DriveItemViewModel(letter, kind, drive.VolumeLabel, drive.DriveFormat, remote, Ready: true, Unresponsive: false,
                Total: (ulong)drive.TotalSize, Free: (ulong)drive.AvailableFreeSpace);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            // Media can disappear between the readiness check and the size query.
            return new DriveItemViewModel(letter, kind, "", "", remote, Ready: false, Unresponsive: false, Total: 0, Free: 0);
        }
    }

    [LibraryImport("mpr.dll", EntryPoint = "WNetGetConnectionW", StringMarshalling = StringMarshalling.Utf16)]
    private static unsafe partial uint WNetGetConnection(string localName, char* remoteName, ref uint length);

    /// <summary>The \\server\share behind a mapped drive letter, or null when the letter is not a network mapping.</summary>
    private static unsafe string? RemotePath(string letter)
    {
        const uint ErrorSuccess = 0;
        uint length = 1024;
        char[] buffer = new char[length];
        fixed (char* remote = buffer)
        {
            return WNetGetConnection(letter, remote, ref length) == ErrorSuccess ? new string(remote) : null;
        }
    }
}
