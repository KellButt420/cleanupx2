using System.Net.Http;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Cleanup.Services;

/// <summary>
/// Finds a newer desktop release on GitHub. Runs only when the person asks;
/// the app makes no background network requests.
/// </summary>
internal static partial class UpdateCheck
{
    public sealed record Release(Version Version, string Tag, Uri Page);

    [GeneratedRegex(@"^v?(\d+\.\d+(?:\.\d+){0,2})(?:[-+].*)?$")]
    private static partial Regex TagVersion();

    /// <summary>The newest published release newer than <paramref name="current"/>, or null when up to date.</summary>
    public static async Task<Release?> FindNewerAsync(Uri repository, Version current, CancellationToken cancellation = default)
    {
        string[] path = repository.AbsolutePath.Trim('/').Split('/');
        if (repository.Host != "github.com" || path.Length != 2) throw new InvalidOperationException("Releases are not hosted on GitHub.");
        using var client = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
        client.DefaultRequestHeaders.UserAgent.ParseAdd($"cleanup/{current.ToString(3)}");
        client.DefaultRequestHeaders.Accept.ParseAdd("application/vnd.github+json");
        await using Stream body = await client.GetStreamAsync(
            $"https://api.github.com/repos/{path[0]}/{path[1]}/releases?per_page=20", cancellation);
        using JsonDocument releases = await JsonDocument.ParseAsync(body, cancellationToken: cancellation);

        Release? newest = null;
        foreach (JsonElement release in releases.RootElement.EnumerateArray())
        {
            if (release.GetProperty("draft").GetBoolean()) continue;
            string tag = release.GetProperty("tag_name").GetString() ?? "";
            Match match = TagVersion().Match(tag);
            if (!match.Success || !Uri.TryCreate(release.GetProperty("html_url").GetString(), UriKind.Absolute, out Uri? page)) continue;
            Version version = Normalize(Version.Parse(match.Groups[1].Value));
            if (newest is null || version > newest.Version) newest = new Release(version, tag, page);
        }
        return newest is not null && newest.Version > Normalize(current) ? newest : null;
    }

    // Compare major.minor.patch only; tags such as v0.1.0.0 carry a zero revision.
    private static Version Normalize(Version version) => new(version.Major, version.Minor, Math.Max(version.Build, 0));
}
