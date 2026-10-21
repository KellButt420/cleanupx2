using Cleanup.Interop;

namespace Cleanup.ViewModels;

/// <summary>Cheap summary data for a virtualized row. Full item details are rendered only on selection.</summary>
public sealed class HistoryItemViewModel
{
    public HistoryItemViewModel(HistoryEntry entry)
    {
        Entry = entry;
        try { Timestamp = DateTimeOffset.FromUnixTimeMilliseconds(checked((long)entry.StartedAtUnixMs)).ToLocalTime(); }
        catch (ArgumentOutOfRangeException) { }
        catch (OverflowException) { }
    }
    public HistoryEntry Entry { get; }
    public DateTimeOffset? Timestamp { get; }
    public string Title => Entry.Title;
    public string Date => Timestamp?.ToString("g") ?? "Date unavailable";
    public string ShortDate => Timestamp is { } date && date.Date == DateTime.Today ? $"Today · {date:t}" : Date;
    public string Kind => Entry.Kind switch { "organize" => "File moves", "uninstall" => "App removal", "leftovers" => "Leftover cleanup", _ => "Operation" };
    public string Glyph => Entry.Kind switch { "organize" => "\uE8B7", "uninstall" => "\uE74D", "leftovers" => "\uE75C", _ => "\uE81C" };
    public string Status => Entry.Status switch { "completed" => "Completed", "completedWithErrors" => "Needs review", "interrupted" => "Interrupted", "undone" => "Restored", "partiallyUndone" => "Partly restored", _ => Entry.Status };
    public bool NeedsAttention => Entry.Status is "completedWithErrors" or "interrupted" or "partiallyUndone";
    public string Counts => Entry.Status == "undone" ? $"{Entry.Undone} restored" :
        string.Join(" · ", new[] { Entry.Succeeded > 0 ? $"{Entry.Succeeded} completed" : null, Entry.Failed > 0 ? $"{Entry.Failed} failed" : null, Entry.Undone > 0 ? $"{Entry.Undone} restored" : null }.OfType<string>());
    public string Summary => string.Join(" · ", new[] { Kind, Counts }.Where(s => s.Length > 0));
    public string Guidance => Entry.Kind switch {
        "uninstall" => "App removal cannot be undone here.",
        "leftovers" => "Recycled files may be in the Recycle Bin. Registry backups require manual review; they are not automatic restoration.",
        _ => Entry.Status == "undone" ? "The recorded moves have been restored." :
            "Undo restores unchanged files only when their original names are available. Inspect interrupted operations before retrying."
    };
    public string Audit => $"Operation ID: {Entry.BatchId}\nType: {Entry.Kind}\nStatus: {Entry.Status}\nStarted: {Timestamp:O}";
    public string Details => $"{Title}\n{Date}\n{Status} · {Summary}\n\n{Guidance}\n\n{Entry.Message}\n\n" +
        string.Join("\n\n", Entry.Items.Select(item => $"{item.Label}\n{item.Detail}\n{item.Outcome}"));
    public bool Matches(string search) => Title.Contains(search, StringComparison.CurrentCultureIgnoreCase) || Kind.Contains(search, StringComparison.CurrentCultureIgnoreCase) || Status.Contains(search, StringComparison.CurrentCultureIgnoreCase);
    public override string ToString() => $"{Title}, {Status}, {Summary}, {Date}";
}
