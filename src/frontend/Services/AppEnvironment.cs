using System.Runtime.InteropServices;

namespace Cleanup.Services;

/// <summary>Package identity and writable data locations for the running app.</summary>
internal static class AppEnvironment
{
    public static string? PackageFullName { get; } = TryGetPackageFullName();

    public static bool IsPackaged => PackageFullName is not null;

    public static string? InstalledPath { get; } = IsPackaged ? Windows.ApplicationModel.Package.Current.InstalledPath : AppContext.BaseDirectory;
    public static string InstanceName => IsPackaged ? Windows.ApplicationModel.Package.Current.Id.FamilyName : "Cleanup.Desktop";

    /// <summary>
    /// Per-user writable state. Inside the package this is the package's
    /// LocalState folder; desktop releases use %LOCALAPPDATA%\Cleanup.
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
#if DEBUG
            "Cleanup.Dev");
#else
            "Cleanup");
#endif
        Directory.CreateDirectory(directory);
        return directory;
    }
}
