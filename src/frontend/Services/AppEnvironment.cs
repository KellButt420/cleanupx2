using System.Runtime.InteropServices;

namespace Cleanup.Services;

/// <summary>Package identity and writable data locations for the running app.</summary>
internal static class AppEnvironment
{
    public static string? PackageFullName { get; } = TryGetPackageFullName();

    public static bool IsPackaged => PackageFullName is not null;

    public static string? InstalledPath { get; } = IsPackaged ? Windows.ApplicationModel.Package.Current.InstalledPath : null;

    /// <summary>
    /// Per-user writable state. Inside the package this is the package's
    /// LocalState folder; unpackaged development runs use %LOCALAPPDATA%\Cleanup.Dev.
    /// The app never writes to its install directory.
    /// </summary>
    public static string LocalStateDirectory { get; } = ResolveLocalState();

    private static string? TryGetPackageFullName()
    {
        try
        {
            return Windows.ApplicationModel.Package.Current.Id.FullName;
        }
        catch (Exception e) when (e is InvalidOperationException or COMException)
        {
            return null;
        }
    }

    private static string ResolveLocalState()
    {
        if (IsPackaged)
        {
            return Windows.Storage.ApplicationData.Current.LocalFolder.Path;
        }
        string directory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Cleanup.Dev");
        Directory.CreateDirectory(directory);
        return directory;
    }
}
