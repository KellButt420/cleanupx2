using System.Text.Json;

namespace Cleanup.Services;

/// <summary>Public launch links shared with the website; never contains credentials.</summary>
internal static class ReleaseLinks
{
    private static readonly Dictionary<string, string> Links = Load();
    public static Uri? Repository => Get("repositoryUrl");
    public static Uri? Discord => Get("discordUrl");
    public static Uri? Website => Get("siteUrl");
    public static Uri? Releases => Repository is { } repository ? new Uri(repository.AbsoluteUri.TrimEnd('/') + "/releases") : null;

    private static Uri? Get(string key) => Links.TryGetValue(key, out string? value) &&
        Uri.TryCreate(value, UriKind.Absolute, out Uri? uri) && uri.Scheme == Uri.UriSchemeHttps && string.IsNullOrEmpty(uri.UserInfo) ? uri : null;

    private static Dictionary<string, string> Load()
    {
        try
        {
            return JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "release.json"))) ?? [];
        }
        catch (Exception error) when (error is IOException or JsonException or UnauthorizedAccessException)
        {
            return [];
        }
    }
}
