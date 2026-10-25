using System.Net.Http;
using System.Text.Json;

namespace Cleanup.Services;

/// <summary>
/// Finds a newer desktop release on GitHub. Runs only when the person asks;
/// the app makes no background network requests.
/// </summary>
internal static class UpdateCheck
{
    public sealed record Release(ReleaseVersion Version, string Tag, Uri Page);

    /// <summary>The newest published release newer than <paramref name="current"/>, or null when up to date.</summary>
    public static async Task<Release?> FindNewerAsync(Uri repository, ReleaseVersion current, CancellationToken cancellation = default)
    {
        string[] path = repository.AbsolutePath.Trim('/').Split('/');
        if (repository.Host != "github.com" || path.Length != 2) throw new InvalidOperationException("Releases are not hosted on GitHub.");
        using var client = new HttpClient { Timeout = TimeSpan.FromSeconds(15), MaxResponseContentBufferSize = 1024 * 1024 };
        client.DefaultRequestHeaders.UserAgent.ParseAdd($"cleanup/{current}");
        client.DefaultRequestHeaders.Accept.ParseAdd("application/vnd.github+json");
        byte[] body = await client.GetByteArrayAsync(
            $"https://api.github.com/repos/{path[0]}/{path[1]}/releases?per_page=100", cancellation);
        using JsonDocument releases = JsonDocument.Parse(body);
        return SelectNewer(releases.RootElement, repository, current);
    }

    internal static Release? SelectNewer(JsonElement releases, Uri repository, ReleaseVersion current)
    {

        Release? newest = null;
        foreach (JsonElement release in releases.EnumerateArray())
        {
            if (release.GetProperty("draft").GetBoolean()) continue;
            string tag = release.GetProperty("tag_name").GetString() ?? "";
            ReleaseVersion? version = ReleaseVersion.Parse(tag);
            if (version is null || (current.Prerelease is null && (version.Prerelease is not null || release.GetProperty("prerelease").GetBoolean()))) continue;
            if (!Uri.TryCreate(release.GetProperty("html_url").GetString(), UriKind.Absolute, out Uri? page) || page.Scheme != "https" || page.Host != "github.com" || !page.AbsolutePath.StartsWith(repository.AbsolutePath.TrimEnd('/') + "/releases/", StringComparison.OrdinalIgnoreCase)) continue;
            if (newest is null || version.CompareTo(newest.Version) > 0) newest = new Release(version, tag, page);
        }
        return newest is not null && newest.Version.CompareTo(current) > 0 ? newest : null;
    }

}
