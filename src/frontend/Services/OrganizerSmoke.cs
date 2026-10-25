using Cleanup.Interop;
using System.Text.Json.Nodes;
namespace Cleanup.Services;

internal static class OrganizerSmoke
{
    public static async Task<JsonObject> RunAsync(string fixtureDirectory)
    {
        string root = Path.Combine(Path.GetFullPath(fixtureDirectory), "organizer-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        string file = Path.Combine(root, "fixture.txt");
        await File.WriteAllTextAsync(file, "cleanup organizer smoke fixture");
        OrganizePlan plan = await Features.PlanAsync(root, "type", "none");
        if (plan.Operations.Length != 1) throw new IOException("Organizer preview did not find exactly the disposable fixture.");
        OrganizeApplyResult applied = await Features.ApplyAsync(plan, plan.Operations, "Release smoke fixture");
        if (applied.Moved != 1 || File.Exists(file)) throw new IOException("Production organizer did not move the fixture.");
        UndoResult undone = await Features.UndoAsync(applied.BatchId);
        if (undone.Restored != 1 || await File.ReadAllTextAsync(file) != "cleanup organizer smoke fixture") throw new IOException("Production organizer did not restore the fixture.");
        // Delete only the unique directory this invocation created, after restoration.
        Directory.Delete(root, recursive: true);
        return new JsonObject { ["moved"] = applied.Moved, ["restored"] = undone.Restored, ["batchId"] = applied.BatchId };
    }
}
