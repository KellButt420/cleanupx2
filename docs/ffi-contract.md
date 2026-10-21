# Native boundary (ABI 2, x64)

The native library is `cleanup_native.dll`; C declarations are in
`src/rust/crates/ffi/include/cleanup.h`. C# uses source-generated `LibraryImport`
with Cdecl and a `SafeHandle` for each operation. DLL search is restricted to the
application assembly directory and System32.

- Check `cln_abi_version() == 2` before other calls.
- Requests and responses are length-delimited UTF-8 JSON, without a terminating NUL.
- Input memory is borrowed during the call; Rust copies/parses it before returning.
- A returned buffer contains a pointer and `size_t` byte count, not a Rust collection.
  Copy it and release it exactly once with `cln_buffer_free`, including error buffers.
- Initialize output buffers to zero. Only the message status produces a message buffer
  from `cln_operation_next`; timeout/complete do not populate one.
- Free each operation exactly once. Calls may poll/cancel concurrently; freeing must
  never race with either. C# disposes the cancellation registration before the handle.
- The caller must provide valid pointers and lengths. The boundary handles null,
  malformed, and oversized requests, but cannot make arbitrary invalid pointers safe.
- Requests are bounded to 4 MiB. Start requests reject unknown JSON fields.
- Exports and workers catch Rust panics. Process aborts and allocator exhaustion are
  outside the recoverable error contract.

Statuses: `0` success, `1` message, `2` timeout, `3` complete, `-1` invalid argument,
`-2` failure, `-3` caught panic. Errors use stable `code`, human `message`, and optional
`win32Error`. Workers emit an error message before completion on failure.

Inventory request: `{"includeMsix":true,"batchSize":200}`. The worker emits zero or
more `{"type":"batch","items":[...]}` records followed by
`{"type":"result","value":{...}}` containing source errors, counts, and cancellation.
Individual source failures are retained rather than disguised as an empty inventory.

Uninstall request: `{"appId":"...","allowElevation":false,"ownerWindow":0}`.
The backend re-reads the installation source before acting; the caller cannot supply
an arbitrary command. Progress messages name validating/running/verifying stages;
the final result distinguishes removed, restart required, cancelled, failed, rejected,
still installed, uncertain, not installed, and detached.

Discovery cancellation stops cooperatively between Windows API calls. Uninstall
cancellation detaches the wait; it does not terminate an installer or promise that
the uninstall was cancelled. UI and future queues must preserve that distinction.
An elevated uninstaller's descendants may be untrackable; primary-process exit then
requires source verification and can yield an uncertain result.

Organizer planning and application also support opaque cancellable operations through
`cln_organize_plan_start` and `cln_organize_apply_start`, using the same result/error
message protocol. Cancellation is observed between files; completed moves remain in
History and can be undone if their file identities still match. Undo, leftover review,
and History exports run on managed thread-pool threads. A single Windows API call
cannot always be interrupted immediately.

Each approved file move carries the identity captured during preview (volume, file
index, size, and last-write ticks). The core denies concurrent writes while checking
and moving the file, holds directory ancestors against replacement, and never changes
the approved destination after confirmation. New conflicts require another preview.

The synchronous persistence and probe-move exports remain Phase 0 diagnostics. They operate
on fixture files and a diagnostic launch log, not user organization rules or a durable
transaction journal. C# runs their filesystem work on a background thread.
