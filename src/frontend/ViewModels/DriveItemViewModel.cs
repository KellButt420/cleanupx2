namespace Cleanup.ViewModels;

internal enum DriveKind { Fixed, Removable, Network, Optical, Ram, Other }

/// <summary>One drive at the moment it was read, with the text the Core page shows for it.</summary>
internal sealed record DriveItemViewModel(
    string Letter,
    DriveKind Kind,
    string Label,
    string FileSystem,
    string? RemotePath,
    bool Ready,
    bool Unresponsive,
    ulong Total,
    ulong Free)
{
    /// <summary>Only ready local, removable and network drives add to the combined totals.</summary>
    public bool CountsTowardTotals => Ready && Total > 0 && (Kind is DriveKind.Fixed or DriveKind.Removable or DriveKind.Network);

    public ulong Used => Total > Free ? Total - Free : 0;

    public double UsedPercent => Total == 0 ? 0 : 100.0 * Used / Total;

    public bool ShowsBar => Ready && Total > 0;

    public string Glyph => Kind switch
    {
        DriveKind.Removable => "",
        DriveKind.Network => "",
        DriveKind.Optical => "",
        _ => "",
    };

    public string KindName => Kind switch
    {
        DriveKind.Fixed => "Local disk",
        DriveKind.Removable => "Removable disk",
        DriveKind.Network => "Network drive",
        DriveKind.Optical => "Optical drive",
        DriveKind.Ram => "RAM disk",
        _ => "Drive",
    };

    public string Title => Label.Length > 0 ? $"{Label} ({Letter})" : $"{KindName} ({Letter})";

    public string Subtitle => string.Join(" · ", new[] { Label.Length > 0 ? KindName : null, FileSystem, RemotePath }
        .Where(part => !string.IsNullOrEmpty(part)));

    public string Capacity
    {
        get
        {
            if (Unresponsive) return "Not responding. Check the connection.";
            if (!Ready)
            {
                return Kind switch
                {
                    DriveKind.Optical => "No disc",
                    DriveKind.Removable => "No media",
                    DriveKind.Network => "Disconnected",
                    _ => "Not ready",
                };
            }
            if (Total == 0) return "No capacity reported";
            return $"{AppItemViewModel.FormatSize(Free) ?? "0 B"} free of {AppItemViewModel.FormatSize(Total) ?? "0 B"}";
        }
    }

    public string AutomationName => $"{Title}. {Capacity}";
}

/// <summary>Combined capacity across the drives that count toward the totals.</summary>
internal sealed record StorageTotals(ulong Total, ulong Free, int Drives, int NetworkDrives)
{
    public ulong Used => Total > Free ? Total - Free : 0;

    public double UsedFraction => Total == 0 ? 0 : (double)Used / Total;

    public static StorageTotals From(IEnumerable<DriveItemViewModel> drives)
    {
        // A network share mapped to two letters reports the same space twice; count it once.
        DriveItemViewModel[] counted = drives
            .Where(drive => drive.CountsTowardTotals)
            .DistinctBy(drive => drive.Kind == DriveKind.Network && drive.RemotePath is { } path ? path : drive.Letter)
            .ToArray();
        // LINQ has no unsigned Sum overload, so the byte counts are added in a loop.
        ulong total = 0, free = 0;
        foreach (DriveItemViewModel drive in counted)
        {
            total += drive.Total;
            free += drive.Free;
        }
        return new StorageTotals(total, free, counted.Length, counted.Count(drive => drive.Kind == DriveKind.Network));
    }
}
