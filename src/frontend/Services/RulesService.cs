using System.Text.Json;
using System.Text.Json.Serialization;
namespace Cleanup.Services;

/// <param name="GroupBy">Legacy index (0 type, 1 extension, 2 date) used when <paramref name="Mode"/> is absent.</param>
/// <param name="Mode">"simple", "type", "extension", or "date".</param>
public sealed record OrganizationRule(string Name, string Source, string Destination, int GroupBy, int DateFolders, int Category, string Extension, int Collision, string? Mode = null, int SchemaVersion = 0)
{
    public string EffectiveMode => Mode ?? GroupBy switch { 1 => "extension", 2 => "date", _ => "type" };

    public override string ToString() => Name;
}
[JsonSerializable(typeof(List<OrganizationRule>))]
internal sealed partial class RulesJsonContext : JsonSerializerContext;

internal static class RulesService
{
    private static readonly string RulesPath = Path.Combine(AppEnvironment.LocalStateDirectory, "organization-rules.json");
    public static async Task<List<OrganizationRule>> LoadAsync()
    {
        try { return JsonSerializer.Deserialize(await File.ReadAllTextAsync(RulesPath), RulesJsonContext.Default.ListOrganizationRule) ?? []; }
        catch (FileNotFoundException) { return []; }
    }
    public static async Task SaveAsync(List<OrganizationRule> rules)
    {
        string temporary = RulesPath + ".tmp";
        await File.WriteAllTextAsync(temporary, JsonSerializer.Serialize(rules, RulesJsonContext.Default.ListOrganizationRule));
        File.Move(temporary, RulesPath, overwrite: true);
    }
}
