using System.Reflection;
using System.Text.RegularExpressions;
namespace Cleanup.Services;

internal sealed partial record ReleaseVersion(int Major, int Minor, int Patch, string? Prerelease) : IComparable<ReleaseVersion>
{
    [GeneratedRegex(@"^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\.0)?(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z.-]+)?$")]
    private static partial Regex Pattern();
    public static ReleaseVersion? Parse(string text)
    {
        Match match = Pattern().Match(text);
        if (!match.Success || !int.TryParse(match.Groups[1].Value, out int major) || !int.TryParse(match.Groups[2].Value, out int minor) || !int.TryParse(match.Groups[3].Value, out int patch)) return null;
        string? pre = match.Groups[4].Success ? match.Groups[4].Value : null;
        if (pre?.Split('.').Any(part => part.Length > 1 && part[0] == '0' && part.All(char.IsAsciiDigit)) == true) return null;
        return new(major, minor, patch, pre);
    }
    public static ReleaseVersion FromAssembly(Assembly assembly) => Parse(assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion ?? "") ?? new(0, 0, 0, "dev");
    public int CompareTo(ReleaseVersion? other)
    {
        if (other is null) return 1;
        int result = Major.CompareTo(other.Major);
        if (result == 0) result = Minor.CompareTo(other.Minor);
        if (result == 0) result = Patch.CompareTo(other.Patch);
        if (result != 0) return result;
        if (Prerelease is null) return other.Prerelease is null ? 0 : 1;
        if (other.Prerelease is null) return -1;
        string[] left = Prerelease.Split('.'), right = other.Prerelease.Split('.');
        for (int i = 0; i < Math.Min(left.Length, right.Length); i++)
        {
            bool ln = left[i].All(char.IsAsciiDigit), rn = right[i].All(char.IsAsciiDigit);
            result = ln && rn ? left[i].Length.CompareTo(right[i].Length) : ln != rn ? (ln ? -1 : 1) : 0;
            if (result == 0) result = string.CompareOrdinal(left[i], right[i]);
            if (result != 0) return result;
        }
        return left.Length.CompareTo(right.Length);
    }
    public override string ToString() => $"{Major}.{Minor}.{Patch}" + (Prerelease is null ? "" : $"-{Prerelease}");
}
