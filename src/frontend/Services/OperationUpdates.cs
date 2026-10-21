using Cleanup.Interop;

namespace Cleanup.Services;

/// <summary>UI-thread notifications between the cached pages. No polling or file watcher.</summary>
internal static class OperationUpdates
{
    public static event Action<string, UndoResult>? UndoCompleted;
    public static void PublishUndo(string batchId, UndoResult result) => UndoCompleted?.Invoke(batchId, result);
}
