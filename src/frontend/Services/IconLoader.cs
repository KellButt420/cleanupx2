using Cleanup.Interop;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Storage;
using Windows.Storage.FileProperties;

namespace Cleanup.Services;

/// <summary>
/// Loads application icons on request, with a small concurrency limit. It keeps no
/// bitmaps itself: the caller owns each icon and decides how long to keep it.
/// Missing or unreadable icons resolve to null and the UI shows a letter avatar instead.
/// </summary>
internal static class IconLoader
{
    private static readonly SemaphoreSlim Gate = new(4);

    public static Task<ImageSource?> LoadAsync(AppRecord app)
    {
        string? path = ResolvePath(app);
        return path is null ? Task.FromResult<ImageSource?>(null) : LoadFileAsync(path);
    }

    /// <summary>Turns a DisplayIcon value ("C:\app.exe,0") or package logo URI into a local file path.</summary>
    internal static string? ResolvePath(AppRecord app)
    {
        string? source = app.IconSource?.Trim();
        if (string.IsNullOrEmpty(source))
        {
            return null;
        }
        if (source.StartsWith("file:", StringComparison.OrdinalIgnoreCase))
        {
            return Uri.TryCreate(source, UriKind.Absolute, out Uri? uri) && !uri.IsUnc ? uri.LocalPath : null;
        }

        source = source.Trim('"');
        int comma = source.LastIndexOf(',');
        if (comma > 2 && int.TryParse(source[(comma + 1)..].Trim(), out _))
        {
            source = source[..comma].Trim().Trim('"');
        }
        source = Environment.ExpandEnvironmentVariables(source);
        return Path.IsPathFullyQualified(source) && !source.StartsWith(@"\\", StringComparison.Ordinal) ? source : null;
    }

    private static async Task<ImageSource?> LoadFileAsync(string path)
    {
        await Gate.WaitAsync();
        try
        {
            string? file = await Task.Run(() => FindExisting(path));
            if (file is null)
            {
                return null;
            }
            StorageFile storageFile = await StorageFile.GetFileFromPathAsync(file);
            using StorageItemThumbnail thumbnail = await storageFile.GetThumbnailAsync(ThumbnailMode.SingleItem, 32, ThumbnailOptions.UseCurrentScale);
            if (thumbnail is null)
            {
                return null;
            }
            var bitmap = new BitmapImage();
            bitmap.DecodePixelWidth = 40;
            await bitmap.SetSourceAsync(thumbnail);
            return bitmap;
        }
        catch (Exception)
        {
            return null;
        }
        finally
        {
            Gate.Release();
        }
    }

    /// <summary>
    /// Package logos are often stored only as scale- or size-qualified variants
    /// (Logo.scale-100.png) of the declared name (Logo.png).
    /// </summary>
    private static string? FindExisting(string path)
    {
        if (File.Exists(path))
        {
            return path;
        }
        string? directory = Path.GetDirectoryName(path);
        if (directory is null || !Directory.Exists(directory))
        {
            return null;
        }
        string stem = Path.GetFileNameWithoutExtension(path);
        string extension = Path.GetExtension(path);
        return Directory.EnumerateFiles(directory, $"{stem}.*{extension}")
            .OrderBy(f => f.Contains("targetsize-32", StringComparison.OrdinalIgnoreCase) ? 0
                : f.Contains("scale-100", StringComparison.OrdinalIgnoreCase) ? 1
                : f.Contains("contrast", StringComparison.OrdinalIgnoreCase) ? 3 : 2)
            .FirstOrDefault();
    }
}
