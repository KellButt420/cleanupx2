namespace Cleanup.Services;

/// <summary>
/// The portable Cleanup.exe is a .NET single-file bundle that extracts its
/// WinUI and native files to %TEMP%\.net\Cleanup\&lt;bundle id&gt;. Each version
/// uses a new bundle id, so older extractions (about 150 MB each) are removed
/// once a newer version is running. Installed (Setup) builds are unaffected.
/// </summary>
internal static class ExtractionCache
{
    public static void RemoveStaleInBackground() => Task.Run(RemoveStale);

    private static void RemoveStale()
    {
        try
        {
            string current = Path.TrimEndingDirectorySeparator(AppContext.BaseDirectory);
            string? executableDirectory = Path.GetDirectoryName(Environment.ProcessPath);
            // Running from an install or publish folder rather than a bundle extraction.
            if (executableDirectory is null || string.Equals(Path.GetFullPath(current), Path.GetFullPath(executableDirectory), StringComparison.OrdinalIgnoreCase)) return;
            DirectoryInfo? appCache = Directory.GetParent(current);
            if (appCache is null || !string.Equals(appCache.Name, "Cleanup", StringComparison.OrdinalIgnoreCase)) return;

            foreach (DirectoryInfo sibling in appCache.EnumerateDirectories())
            {
                if (string.Equals(sibling.FullName, current, StringComparison.OrdinalIgnoreCase)) continue;
                // Windows refuses to rename a directory while another running
                // version has files mapped from it, so a cache in use is never
                // partially deleted. Renaming first also hides it from the
                // .NET host before deletion starts.
                string doomed = Path.Combine(appCache.FullName, ".stale-" + Guid.NewGuid().ToString("N"));
                try
                {
                    if (!sibling.Name.StartsWith(".stale-", StringComparison.Ordinal)) sibling.MoveTo(doomed);
                    else doomed = sibling.FullName;
                    Directory.Delete(doomed, recursive: true);
                }
                catch (Exception error) when (error is IOException or UnauthorizedAccessException) { /* In use or locked; retry next launch. */ }
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or ArgumentException) { /* Cache cleanup is best effort. */ }
    }
}
