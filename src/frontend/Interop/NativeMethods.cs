using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

// Load native libraries only from the application directory or System32,
// never from the current directory or PATH.
[assembly: DefaultDllImportSearchPaths(DllImportSearchPath.AssemblyDirectory | DllImportSearchPath.System32)]

namespace Cleanup.Interop;

/// <summary>Byte buffer owned by the native core. Release with <c>cln_buffer_free</c>.</summary>
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeBuffer
{
    public byte* Pointer;
    public nuint Length;
}

/// <summary>Status codes returned by the native core (ABI version 1).</summary>
internal static class NativeStatus
{
    public const int Ok = 0;
    public const int Message = 1;
    public const int Timeout = 2;
    public const int Complete = 3;
    public const int InvalidArgument = -1;
    public const int Failed = -2;
    public const int Panic = -3;
}

/// <summary>Native operation handle, released exactly once with <c>cln_operation_free</c>.</summary>
internal sealed class OperationHandle : SafeHandle
{
    public OperationHandle()
        : base(IntPtr.Zero, ownsHandle: true)
    {
    }

    public override bool IsInvalid => handle == IntPtr.Zero;

    protected override bool ReleaseHandle()
    {
        NativeMethods.cln_operation_free(handle);
        return true;
    }
}

/// <summary>Declarations for <c>cleanup_native.dll</c>. See <c>src/rust/crates/ffi/src/lib.rs</c> for the contract.</summary>
internal static unsafe partial class NativeMethods
{
    [LibraryImport("cleanup_native")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_organize_plan_start(byte* request, nuint requestLength, out OperationHandle operation, NativeBuffer* error);

    [LibraryImport("cleanup_native")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_organize_apply_start(byte* request, nuint requestLength, out OperationHandle operation, NativeBuffer* error);
    private const string Library = "cleanup_native";

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial uint cln_abi_version();

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial void cln_buffer_free(NativeBuffer buffer);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_environment_probe(NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_probe_persistence(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_probe_file_moves(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_inventory_scan_start(byte* request, nuint requestLength, out OperationHandle operation, NativeBuffer* error);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_uninstall_start(byte* request, nuint requestLength, out OperationHandle operation, NativeBuffer* error);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_operation_next(OperationHandle operation, uint timeoutMs, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_operation_cancel(OperationHandle operation);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial void cln_operation_free(nint operation);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_organize_locations(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_organize_plan(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_organize_apply(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_organize_undo(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_leftovers_scan(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_leftovers_remove(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_history_list(byte* request, nuint requestLength, NativeBuffer* output);

    [LibraryImport(Library)]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int cln_history_record_uninstall(byte* request, nuint requestLength, NativeBuffer* output);
}
