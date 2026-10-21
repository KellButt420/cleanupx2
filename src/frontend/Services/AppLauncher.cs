using System.ComponentModel;
using System.Diagnostics;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Xml.Linq;
using Cleanup.Interop;
using Cleanup.ViewModels;

namespace Cleanup.Services;

public enum LaunchKind { Uri, File, Program }

/// <summary>How to open an app or game: a protocol or shell link, a file or shortcut, or a program with arguments.</summary>
public sealed record LaunchTarget(LaunchKind Kind, string Target, string? Arguments = null, string? WorkingDirectory = null);

/// <summary>Opens an app or game from the Applications list the way Start or Explorer would.</summary>
internal static class AppLauncher
{
    private static readonly Lazy<Dictionary<string, string>> ShortcutTargets = new(ReadShortcutTargets);

    /// <summary>Opens the app or game. Returns why it could not be opened, or null once it has started.</summary>
    public static Task<string?> OpenAsync(AppItemViewModel item) => Task.Run(() => Open(item));

    /// <summary>True for an executable a person starts, rather than an uninstaller, installer, updater, or helper.</summary>
    internal static bool IsMainProgram(string fileName) =>
        !HelperWords.Any(word => fileName.Contains(word, StringComparison.OrdinalIgnoreCase));

    private static readonly string[] HelperWords =
        ["unins", "uninst", "setup", "install", "update", "helper", "crash", "redist", "report", "nogui", "cmd", "tool"];

    private static string? Open(AppItemViewModel item)
    {
        LaunchTarget? target;
        try
        {
            target = item.Launch ?? ResolveInstalled(item.Record);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return $"Couldn't read the install folder: {error.Message}";
        }
        if (target is null)
        {
            return "Windows doesn't record a program for this app. Use Open install location to find it.";
        }
        try
        {
            Process.Start(new ProcessStartInfo(target.Target)
            {
                UseShellExecute = true,
                Arguments = target.Arguments ?? "",
                WorkingDirectory = target.WorkingDirectory ?? "",
            })?.Dispose();
            return null;
        }
        catch (Win32Exception error)
        {
            return error.Message;
        }
    }

    private static LaunchTarget? ResolveInstalled(AppRecord record) =>
        record.Technology == "msix" ? PackageTarget(record) : ProgramTarget(record);

    /// <summary>A store app starts through its package and the application ID in its manifest.</summary>
    private static LaunchTarget? PackageTarget(AppRecord record)
    {
        if (string.IsNullOrEmpty(record.PackageFamilyName) || string.IsNullOrEmpty(record.InstallLocation)) return null;
        string manifest = Path.Combine(record.InstallLocation, "AppxManifest.xml");
        if (!File.Exists(manifest)) return null;
        string? applicationId = XDocument.Load(manifest).Descendants()
            .FirstOrDefault(element => element.Name.LocalName == "Application")?.Attribute("Id")?.Value;
        return applicationId is null ? null : new LaunchTarget(LaunchKind.Uri, $"shell:AppsFolder\\{record.PackageFamilyName}!{applicationId}");
    }

    /// <summary>A desktop app starts from the program its installer named, a Start Menu shortcut into its folder, or its main program.</summary>
    private static LaunchTarget? ProgramTarget(AppRecord record)
    {
        string? icon = IconLoader.ResolvePath(record);
        if (icon is not null && icon.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) && File.Exists(icon) && IsMainProgram(Path.GetFileName(icon)))
        {
            return new LaunchTarget(LaunchKind.Program, icon, null, Path.GetDirectoryName(icon));
        }

        string? folder = record.InstallLocation?.TrimEnd('\\');
        if (string.IsNullOrEmpty(folder) || !Directory.Exists(folder)) return null;

        string prefix = folder + Path.DirectorySeparatorChar;
        string firstWord = record.DisplayName.Split(' ', StringSplitOptions.RemoveEmptyEntries).FirstOrDefault() ?? "";
        string? shortcut = ShortcutTargets.Value
            .Where(pair => pair.Value.StartsWith(prefix, StringComparison.OrdinalIgnoreCase) && pair.Value.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
            .OrderByDescending(pair => firstWord.Length > 0 && Path.GetFileNameWithoutExtension(pair.Key).Contains(firstWord, StringComparison.OrdinalIgnoreCase))
            .Select(pair => pair.Key)
            .FirstOrDefault();
        if (shortcut is not null) return new LaunchTarget(LaunchKind.File, shortcut);

        string[] programs = Directory.EnumerateFiles(folder, "*.exe", SearchOption.TopDirectoryOnly)
            .Where(program => IsMainProgram(Path.GetFileName(program)))
            .ToArray();
        string folderName = Path.GetFileName(folder);
        string? main = programs.FirstOrDefault(program => Path.GetFileNameWithoutExtension(program).Equals(folderName, StringComparison.OrdinalIgnoreCase))
            ?? (programs.Length == 1 ? programs[0] : null);
        return main is null ? null : new LaunchTarget(LaunchKind.Program, main, null, folder);
    }

    /// <summary>Maps each Start Menu shortcut to the file it starts. Read once, the first time it is needed.</summary>
    private static Dictionary<string, string> ReadShortcutTargets()
    {
        var targets = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        Type? shellType = Type.GetTypeFromProgID("WScript.Shell");
        if (shellType is null) return targets;

        object shell = Activator.CreateInstance(shellType)!;
        string[] folders = [Environment.GetFolderPath(Environment.SpecialFolder.CommonPrograms), Environment.GetFolderPath(Environment.SpecialFolder.Programs)];
        foreach (string folder in folders.Where(Directory.Exists).Distinct(StringComparer.OrdinalIgnoreCase))
        {
            foreach (string link in Directory.EnumerateFiles(folder, "*.lnk", SearchOption.AllDirectories))
            {
                try
                {
                    object shortcut = shellType.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, shell, [link])!;
                    if (shortcut.GetType().InvokeMember("TargetPath", BindingFlags.GetProperty, null, shortcut, null) is string target && target.Length > 0)
                    {
                        targets[link] = target;
                    }
                }
                catch (Exception error) when (error is COMException or TargetInvocationException or IOException or UnauthorizedAccessException)
                {
                    // A shortcut that can't be read is simply not used.
                }
            }
        }
        return targets;
    }
}
