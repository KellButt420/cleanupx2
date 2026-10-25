using System.Text;
using System.Text.Json;

namespace Cleanup.Interop;

internal sealed class NativeCoreException(string code, string message, int status) : Exception(message)
{
    public string Code { get; } = code;

    public int Status { get; } = status;

    public uint? Win32Error { get; init; }
}

internal sealed record InventoryScanResult(IReadOnlyList<AppRecord> Records, string SummaryJson, int Batches);

/// <summary>
/// Managed wrapper over the native core. Owns every native buffer it receives,
/// keeps all blocking native calls off the UI thread, and turns native errors
/// into <see cref="NativeCoreException"/>.
/// </summary>
internal static unsafe class NativeCore
{
    internal static Task<string> RunFeatureAsync(byte[] request, nint function, CancellationToken cancellation)
    {
        OperationHandle handle = Start(request, (delegate*<byte*, nuint, out OperationHandle, NativeBuffer*, int>)function);
        return Task.Run(() =>
        {
            string? result = null;
            Pump(handle, cancellation, message =>
            {
                if (message.GetProperty("type").GetString() == "result") result = message.GetProperty("value").GetRawText();
                else ThrowOperationError(message);
            });
            return result ?? throw new NativeCoreException("noResult", "The operation produced no result.", NativeStatus.Failed);
        });
    }
    public const uint ExpectedAbiVersion = 3;

    private const uint PollTimeoutMs = 100;

    public static uint AbiVersion => NativeMethods.cln_abi_version();

    /// <summary>Loads the library and verifies the ABI version.</summary>
    public static void EnsureCompatible()
    {
        uint actual = AbiVersion;
        if (actual != ExpectedAbiVersion)
        {
            throw new NativeCoreException(
                "abiMismatch",
                $"The native core reports ABI version {actual}; this build expects {ExpectedAbiVersion}.",
                NativeStatus.Failed);
        }
    }

    public static string ProbeEnvironment()
    {
        EnsureCompatible();
        NativeBuffer output = default;
        int status = NativeMethods.cln_environment_probe(&output);
        return Complete(status, ref output);
    }

    public static string ProbePersistence(string directory) =>
        CallWithRequest(Serialize(new DirectoryRequest(directory)), &NativeMethods.cln_probe_persistence);

    public static string ProbeFileMoves(string directory) =>
        CallWithRequest(Serialize(new DirectoryRequest(directory)), &NativeMethods.cln_probe_file_moves);

    /// <summary>
    /// Streams the installed-application inventory. <paramref name="onBatch"/>
    /// runs on a background thread. Cancelling stops the scan early; the
    /// partial result is still returned with <c>cancelled: true</c> in the summary.
    /// </summary>
    public static Task<InventoryScanResult> ScanInventoryAsync(
        Action<IReadOnlyList<AppRecord>>? onBatch,
        CancellationToken cancellationToken)
    {
        byte[] request = JsonSerializer.SerializeToUtf8Bytes(new ScanRequest(true, 200), CoreJsonContext.Default.ScanRequest);
        OperationHandle handle = Start(request, &NativeMethods.cln_inventory_scan_start);
        return Task.Factory.StartNew(
            () =>
            {
                var records = new List<AppRecord>();
                string summary = "{}";
                int batches = 0;
                Pump(handle, cancellationToken, message =>
                {
                    switch (message.GetProperty("type").GetString())
                    {
                        case "batch":
                            AppRecord[] items = message.GetProperty("items").Deserialize(CoreJsonContext.Default.AppRecordArray) ?? [];
                            records.AddRange(items);
                            batches++;
                            onBatch?.Invoke(items);
                            break;
                        case "result":
                            summary = message.GetProperty("value").GetRawText();
                            break;
                        default:
                            ThrowOperationError(message);
                            break;
                    }
                });
                return new InventoryScanResult(records, summary, batches);
            },
            CancellationToken.None,
            TaskCreationOptions.LongRunning,
            TaskScheduler.Default);
    }

    /// <summary>
    /// Runs the official uninstall mechanism for <paramref name="appId"/> and
    /// returns the native report JSON. Cancelling stops waiting; it never
    /// terminates the uninstaller.
    /// </summary>
    public static Task<string> UninstallAsync(
        string appId,
        bool allowElevation,
        nint ownerWindow,
        Action<string>? onStage,
        CancellationToken cancellationToken)
    {
        byte[] request = JsonSerializer.SerializeToUtf8Bytes(
            new UninstallRequest(appId, allowElevation, ownerWindow),
            CoreJsonContext.Default.UninstallRequest);
        OperationHandle handle = Start(request, &NativeMethods.cln_uninstall_start);
        return Task.Factory.StartNew(
            () =>
            {
                string? result = null;
                Pump(handle, cancellationToken, message =>
                {
                    switch (message.GetProperty("type").GetString())
                    {
                        case "progress":
                            onStage?.Invoke(message.GetProperty("stage").GetString() ?? "");
                            break;
                        case "result":
                            result = message.GetProperty("value").GetRawText();
                            break;
                        default:
                            ThrowOperationError(message);
                            break;
                    }
                });
                return result ?? throw new NativeCoreException("noResult", "The uninstall operation ended without a result.", NativeStatus.Failed);
            },
            CancellationToken.None,
            TaskCreationOptions.LongRunning,
            TaskScheduler.Default);
    }

    private static byte[] Serialize(DirectoryRequest request) =>
        JsonSerializer.SerializeToUtf8Bytes(request, CoreJsonContext.Default.DirectoryRequest);

    /// <summary>Synchronous JSON call; callers must already be off the UI thread.</summary>
    internal static string Call(byte[] request, delegate*<byte*, nuint, NativeBuffer*, int> function) => CallWithRequest(request, function);

    private static string CallWithRequest(byte[] request, delegate*<byte*, nuint, NativeBuffer*, int> function)
    {
        EnsureCompatible();
        NativeBuffer output = default;
        int status;
        fixed (byte* pointer = request)
        {
            status = function(pointer, (nuint)request.Length, &output);
        }
        return Complete(status, ref output);
    }

    private static OperationHandle Start(
        byte[] request,
        delegate*<byte*, nuint, out OperationHandle, NativeBuffer*, int> function)
    {
        EnsureCompatible();
        NativeBuffer error = default;
        OperationHandle handle;
        int status;
        fixed (byte* pointer = request)
        {
            status = function(pointer, (nuint)request.Length, out handle, &error);
        }
        string errorJson = Take(ref error);
        if (status != NativeStatus.Ok || handle.IsInvalid)
        {
            handle.Dispose();
            throw ToException(status, errorJson);
        }
        return handle;
    }

    /// <summary>Pulls messages until the operation completes, then frees the handle.</summary>
    private static void Pump(OperationHandle handle, CancellationToken cancellationToken, Action<JsonElement> onMessage)
    {
        using (handle)
        using (cancellationToken.Register(static state => NativeMethods.cln_operation_cancel((OperationHandle)state!), handle))
        {
            while (true)
            {
                NativeBuffer buffer = default;
                int status = NativeMethods.cln_operation_next(handle, PollTimeoutMs, &buffer);
                switch (status)
                {
                    case NativeStatus.Message:
                        using (JsonDocument document = JsonDocument.Parse(Take(ref buffer)))
                        {
                            onMessage(document.RootElement);
                        }
                        break;
                    case NativeStatus.Timeout:
                        break;
                    case NativeStatus.Complete:
                        return;
                    default:
                        throw new NativeCoreException("operationFailed", $"Native operation failed with status {status}.", status);
                }
            }
        }
    }

    private static string Complete(int status, ref NativeBuffer buffer)
    {
        string text = Take(ref buffer);
        return status == NativeStatus.Ok ? text : throw ToException(status, text);
    }

    /// <summary>Copies a native buffer into a string and releases it.</summary>
    private static string Take(ref NativeBuffer buffer)
    {
        if (buffer.Pointer == null)
        {
            return string.Empty;
        }
        try
        {
            return Encoding.UTF8.GetString(buffer.Pointer, checked((int)buffer.Length));
        }
        finally
        {
            NativeMethods.cln_buffer_free(buffer);
            buffer = default;
        }
    }

    private static void ThrowOperationError(JsonElement message)
    {
        if (message.TryGetProperty("error", out JsonElement error))
        {
            throw ToException(NativeStatus.Failed, error.GetRawText());
        }
        throw new NativeCoreException("unexpectedMessage", $"Unexpected native message: {message.GetRawText()}", NativeStatus.Failed);
    }

    private static NativeCoreException ToException(int status, string errorJson)
    {
        CoreErrorInfo? error = null;
        if (!string.IsNullOrEmpty(errorJson))
        {
            try
            {
                error = JsonSerializer.Deserialize(errorJson, CoreJsonContext.Default.CoreErrorInfo);
            }
            catch (JsonException)
            {
            }
        }
        return new NativeCoreException(
            error?.Code ?? "unknown",
            error?.Message ?? $"Native call failed with status {status}.",
            status)
        {
            Win32Error = error?.Win32Error,
        };
    }
}
