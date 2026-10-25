//! C ABI for the Cleanup native core.
//!
//! # Contract (ABI version 3)
//!
//! * All requests and responses are UTF-8 JSON.
//! * Input buffers are owned by the caller and only borrowed for the
//!   duration of a call.
//! * Every [`ClnBuffer`] returned through an out parameter is owned by the
//!   caller and must be released exactly once with [`cln_buffer_free`].
//! * Long-running work returns an opaque [`Operation`] handle. Messages are
//!   pulled with [`cln_operation_next`]; [`cln_operation_cancel`] may be called
//!   from any thread; [`cln_operation_free`] must be called exactly once and
//!   not concurrently with other calls on the same handle.
//! * Functions return a `CLN_*` status. On `CLN_E_FAILED` or `CLN_E_PANIC` the
//!   error out buffer (when provided) holds a JSON `CoreError`.
//! * Panics never unwind across the boundary.

mod operation;
mod probes;

use std::panic::{AssertUnwindSafe, catch_unwind};

use cleanup_core::{CoreError, ErrorCode};
use serde::Deserialize;
use serde::Serialize;

pub use operation::Operation;
use operation::{Next, Sink};

pub const CLN_ABI_VERSION: u32 = 3;

macro_rules! cancellable_export {
    ($name:ident, $request:ty, $worker:expr) => {
        /// Starts a cancellable file operation, emitting a result or error message.
        ///
        /// # Safety
        /// Input must contain `request_len` readable bytes. Output pointers must
        /// be valid; `out_error` may be null. Free returned buffers and the
        /// operation handle exactly once under the ABI ownership contract.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(request: *const u8, request_len: usize, out_operation: *mut *mut Operation, out_error: *mut ClnBuffer) -> i32 {
            unsafe { start(out_operation, out_error, || {
                let request: $request = parse_request(request, request_len)?;
                Operation::spawn("cleanup-files", move |sink: &Sink| {
                    let value = $worker(&request, &|| sink.is_cancelled());
                    match value {
                        Ok(value) => sink.send(&serde_json::json!({"type":"result", "value":value})),
                        Err(error) => sink.send(&serde_json::json!({"type":"error", "error":error})),
                    }
                })
            }) }
        }
    };
}

cancellable_export!(
    cln_organize_plan_start,
    cleanup_organizer::PlanRequest,
    cleanup_organizer::plan_with_cancel
);
cancellable_export!(
    cln_organize_apply_start,
    cleanup_organizer::ApplyRequest,
    cleanup_organizer::apply_with_cancel
);

pub const CLN_OK: i32 = 0;
pub const CLN_MESSAGE: i32 = 1;
pub const CLN_TIMEOUT: i32 = 2;
pub const CLN_COMPLETE: i32 = 3;
pub const CLN_E_INVALID_ARGUMENT: i32 = -1;
pub const CLN_E_FAILED: i32 = -2;
pub const CLN_E_PANIC: i32 = -3;

const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;

/// Byte buffer allocated by the core. Release with [`cln_buffer_free`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ClnBuffer {
    pub ptr: *mut u8,
    pub len: usize,
}

impl ClnBuffer {
    const EMPTY: ClnBuffer = ClnBuffer {
        ptr: std::ptr::null_mut(),
        len: 0,
    };

    fn from_vec(bytes: Vec<u8>) -> Self {
        let boxed = bytes.into_boxed_slice();
        let len = boxed.len();
        ClnBuffer {
            ptr: Box::into_raw(boxed).cast::<u8>(),
            len,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn cln_abi_version() -> u32 {
    CLN_ABI_VERSION
}

/// Releases a buffer returned by the core. Null buffers are ignored.
///
/// # Safety
/// `buffer` must have been returned by this library and not freed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_buffer_free(buffer: ClnBuffer) {
    if buffer.ptr.is_null() {
        return;
    }
    let _ = catch_unwind(|| {
        // SAFETY: reconstructs the boxed slice created in `ClnBuffer::from_vec`.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buffer.ptr, buffer.len)) });
    });
}

/// Reports package identity, elevation, module locations, and known folders.
///
/// # Safety
/// `out` must be a valid pointer to a `ClnBuffer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_environment_probe(out: *mut ClnBuffer) -> i32 {
    // SAFETY: forwarded caller contract.
    unsafe { call(out, || to_json(&probes::environment())) }
}

/// Appends a launch record to a journal in `directory` and returns every
/// record so far, demonstrating persistence across restarts.
///
/// # Safety
/// `request` must point to `request_len` readable bytes; `out` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_probe_persistence(
    request: *const u8,
    request_len: usize,
    out: *mut ClnBuffer,
) -> i32 {
    // SAFETY: forwarded caller contract.
    unsafe {
        call(out, || {
            let request: probes::DirectoryRequest = parse_request(request, request_len)?;
            to_json(&probes::persistence(&request)?)
        })
    }
}

/// Creates fixture files inside `directory` and moves them, demonstrating
/// approved moves, refusal to overwrite, and move-back (undo).
///
/// # Safety
/// `request` must point to `request_len` readable bytes; `out` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_probe_file_moves(
    request: *const u8,
    request_len: usize,
    out: *mut ClnBuffer,
) -> i32 {
    // SAFETY: forwarded caller contract.
    unsafe {
        call(out, || {
            let request: probes::DirectoryRequest = parse_request(request, request_len)?;
            to_json(&probes::file_moves(&request)?)
        })
    }
}

/// Generates a synchronous JSON-in / JSON-out export. The caller runs these
/// on a background thread; each finishes in well under a second for typical
/// folders except organize/apply, whose duration depends on the files moved.
macro_rules! json_export {
    ($(#[$doc:meta])* $name:ident, $request:ty, $body:expr) => {
        $(#[$doc])*
        ///
        /// # Safety
        /// `request` must point to `request_len` readable bytes; `out` must be valid.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(request: *const u8, request_len: usize, out: *mut ClnBuffer) -> i32 {
            // SAFETY: forwarded caller contract.
            unsafe {
                call(out, || {
                    let request: $request = parse_request(request, request_len)?;
                    let run: fn(&$request) -> Result<Vec<u8>, CoreError> = $body;
                    run(&request)
                })
            }
        }
    };
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JournalRequest {
    journal_directory: String,
    #[serde(default)]
    before: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordUninstallRequest {
    journal_directory: String,
    app_name: String,
    #[serde(default)]
    publisher: Option<String>,
    status: String,
    #[serde(default)]
    message: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

json_export!(
    /// Downloads, Desktop, and Documents resolved through the known-folder API.
    cln_organize_locations, Empty, |_| to_json(&cleanup_organizer::locations())
);
json_export!(
    /// Dry run: proposed moves, skipped files with reasons, collisions renamed.
    cln_organize_plan, cleanup_organizer::PlanRequest, |r| to_json(&cleanup_organizer::plan(r)?)
);
json_export!(
    /// Performs approved moves, journaling each one; never overwrites.
    cln_organize_apply, cleanup_organizer::ApplyRequest, |r| to_json(&cleanup_organizer::apply(r)?)
);
json_export!(
    /// Moves an organize batch's files back when they are unchanged.
    cln_organize_undo, cleanup_organizer::UndoRequest, |r| to_json(&cleanup_organizer::undo(r)?)
);
json_export!(
    /// Lists leftover candidates for a removed app, with reasons and confidence.
    cln_leftovers_scan, cleanup_leftovers::AppInfo, |r| to_json(&cleanup_leftovers::scan(r))
);
json_export!(
    /// Rescans, then recycles selected files and folders and deletes selected
    /// registry keys after backing them up.
    cln_leftovers_remove, cleanup_leftovers::RemoveRequest, |r| to_json(&cleanup_leftovers::remove(r)?)
);
json_export!(
    /// History entries, newest first, including interrupted batches.
    cln_history_list, JournalRequest, |r| {
        let journal = cleanup_operations::Journal::open(&r.journal_directory)?;
        to_json(&journal.recent(r.before.as_deref())?)
    }
);
json_export!(
    /// Records an uninstall result in history (informational; not reversible).
    cln_history_record_uninstall, RecordUninstallRequest, |r| {
        let journal = cleanup_operations::Journal::open(&r.journal_directory)?;
        journal.append(&cleanup_operations::Event::UninstallRecorded {
            batch_id: cleanup_operations::new_batch_id(),
            at_unix_ms: cleanup_core::time::unix_millis_now(),
            app_name: r.app_name.clone(),
            publisher: r.publisher.clone(),
            status: r.status.clone(),
            message: r.message.clone(),
        })?;
        to_json(&serde_json::json!({}))
    }
);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScanRequest {
    #[serde(default = "default_true")]
    include_msix: bool,
    #[serde(default = "default_batch_size")]
    batch_size: usize,
}

fn default_true() -> bool {
    true
}

fn default_batch_size() -> usize {
    200
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ScanMessage<'a> {
    Batch {
        items: &'a [cleanup_core::model::AppRecord],
    },
    Result {
        value: &'a cleanup_discovery::ScanSummary,
    },
}

/// Starts an inventory scan. Messages: `{"type":"batch","items":[...]}` zero
/// or more times, then `{"type":"result","value":{...summary}}`.
///
/// # Safety
/// `request` must point to `request_len` readable bytes; `out_operation` and
/// `out_error` must be valid pointers (`out_error` may be null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_inventory_scan_start(
    request: *const u8,
    request_len: usize,
    out_operation: *mut *mut Operation,
    out_error: *mut ClnBuffer,
) -> i32 {
    // SAFETY: forwarded caller contract.
    unsafe {
        start(out_operation, out_error, || {
            let request: ScanRequest = parse_request(request, request_len)?;
            let options = cleanup_discovery::ScanOptions {
                include_msix: request.include_msix,
                batch_size: request.batch_size,
            };
            Operation::spawn("cleanup-inventory", move |sink: &Sink| {
                let summary = cleanup_discovery::scan(
                    &options,
                    &mut |batch| sink.send(&ScanMessage::Batch { items: &batch }),
                    &|| sink.is_cancelled(),
                );
                sink.send(&ScanMessage::Result { value: &summary });
            })
        })
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum UninstallMessage<'a> {
    Progress {
        stage: cleanup_uninstall::Stage,
    },
    Result {
        value: &'a cleanup_uninstall::UninstallReport,
    },
}

/// Starts an official uninstall for one application id. Messages:
/// `{"type":"progress","stage":...}` then `{"type":"result","value":{...}}`.
///
/// # Safety
/// Same contract as [`cln_inventory_scan_start`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_uninstall_start(
    request: *const u8,
    request_len: usize,
    out_operation: *mut *mut Operation,
    out_error: *mut ClnBuffer,
) -> i32 {
    // SAFETY: forwarded caller contract.
    unsafe {
        start(out_operation, out_error, || {
            let request: cleanup_uninstall::UninstallRequest = parse_request(request, request_len)?;
            Operation::spawn("cleanup-uninstall", move |sink: &Sink| {
                let report = cleanup_uninstall::uninstall(
                    &request,
                    &mut |stage| sink.send(&UninstallMessage::Progress { stage }),
                    &|| sink.is_cancelled(),
                );
                sink.send(&UninstallMessage::Result { value: &report });
            })
        })
    }
}

/// Waits up to `timeout_ms` for the next message. Returns `CLN_MESSAGE` with
/// the message in `out`, `CLN_TIMEOUT`, or `CLN_COMPLETE` once the operation
/// has finished and every message has been read.
///
/// # Safety
/// `operation` must be a live handle; `out` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_operation_next(
    operation: *mut Operation,
    timeout_ms: u32,
    out: *mut ClnBuffer,
) -> i32 {
    if operation.is_null() || out.is_null() {
        return CLN_E_INVALID_ARGUMENT;
    }
    // SAFETY: caller guarantees the handle is live and not being freed.
    let operation = unsafe { &*operation };
    match catch_unwind(AssertUnwindSafe(|| operation.next(timeout_ms))) {
        Ok(Next::Message(bytes)) => {
            // SAFETY: `out` was checked for null.
            unsafe { *out = ClnBuffer::from_vec(bytes) };
            CLN_MESSAGE
        }
        Ok(Next::Timeout) => CLN_TIMEOUT,
        Ok(Next::Complete) => CLN_COMPLETE,
        Err(_) => CLN_E_PANIC,
    }
}

/// Requests cancellation. Idempotent; never terminates uninstallers.
///
/// # Safety
/// `operation` must be a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_operation_cancel(operation: *mut Operation) -> i32 {
    if operation.is_null() {
        return CLN_E_INVALID_ARGUMENT;
    }
    // SAFETY: caller guarantees the handle is live.
    unsafe { &*operation }.cancel();
    CLN_OK
}

/// Cancels (if still running) and releases an operation handle.
///
/// # Safety
/// `operation` must be null or a live handle that is freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cln_operation_free(operation: *mut Operation) {
    if operation.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: ownership returns to Rust exactly once.
        let operation = unsafe { Box::from_raw(operation) };
        operation.cancel();
        drop(operation);
    }));
}

/// Runs `body`, writing its JSON result or a JSON error into `out`.
unsafe fn call(out: *mut ClnBuffer, body: impl FnOnce() -> Result<Vec<u8>, CoreError>) -> i32 {
    if out.is_null() {
        return CLN_E_INVALID_ARGUMENT;
    }
    // SAFETY: `out` was checked for null.
    unsafe { *out = ClnBuffer::EMPTY };
    let (status, bytes) = match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(bytes)) => (CLN_OK, bytes),
        Ok(Err(error)) => (error_status(&error), error_json(&error)),
        Err(payload) => (CLN_E_PANIC, error_json(&panic_error(payload))),
    };
    // SAFETY: as above.
    unsafe { *out = ClnBuffer::from_vec(bytes) };
    status
}

unsafe fn start(
    out_operation: *mut *mut Operation,
    out_error: *mut ClnBuffer,
    body: impl FnOnce() -> Result<Box<Operation>, CoreError>,
) -> i32 {
    if out_operation.is_null() {
        return CLN_E_INVALID_ARGUMENT;
    }
    // SAFETY: checked for null; `out_error` is optional.
    unsafe {
        *out_operation = std::ptr::null_mut();
        if !out_error.is_null() {
            *out_error = ClnBuffer::EMPTY;
        }
    }
    let (status, error) = match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(operation)) => {
            // SAFETY: checked for null above.
            unsafe { *out_operation = Box::into_raw(operation) };
            return CLN_OK;
        }
        Ok(Err(error)) => (error_status(&error), error),
        Err(payload) => (CLN_E_PANIC, panic_error(payload)),
    };
    if !out_error.is_null() {
        // SAFETY: checked for null.
        unsafe { *out_error = ClnBuffer::from_vec(error_json(&error)) };
    }
    status
}

unsafe fn parse_request<T: for<'de> Deserialize<'de>>(
    ptr: *const u8,
    len: usize,
) -> Result<T, CoreError> {
    if len > MAX_REQUEST_BYTES {
        return Err(CoreError::invalid("Request is too large."));
    }
    let bytes: &[u8] = if len == 0 {
        b"{}"
    } else if ptr.is_null() {
        return Err(CoreError::invalid("Request pointer is null."));
    } else {
        // SAFETY: caller guarantees `len` readable bytes.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    serde_json::from_slice(bytes).map_err(|e| CoreError::invalid(format!("Invalid request: {e}")))
}

fn to_json<T: Serialize>(value: &T) -> Result<Vec<u8>, CoreError> {
    serde_json::to_vec(value).map_err(|e| CoreError::new(ErrorCode::Internal, e.to_string()))
}

fn error_status(error: &CoreError) -> i32 {
    if error.code == ErrorCode::InvalidArgument {
        CLN_E_INVALID_ARGUMENT
    } else {
        CLN_E_FAILED
    }
}

pub(crate) fn error_json(error: &CoreError) -> Vec<u8> {
    serde_json::to_vec(error)
        .unwrap_or_else(|_| br#"{"code":"internal","message":"error"}"#.to_vec())
}

pub(crate) fn panic_error(payload: Box<dyn std::any::Any + Send>) -> CoreError {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into());
    CoreError::new(
        ErrorCode::Internal,
        format!("Internal error in native core: {detail}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(buffer: ClnBuffer) -> String {
        assert!(!buffer.ptr.is_null());
        // SAFETY: buffer returned by the library under test.
        let text = String::from_utf8(
            unsafe { std::slice::from_raw_parts(buffer.ptr, buffer.len) }.to_vec(),
        )
        .unwrap();
        unsafe { cln_buffer_free(buffer) };
        text
    }

    #[test]
    fn buffers_round_trip_and_free() {
        let buffer = ClnBuffer::from_vec(b"hello".to_vec());
        assert_eq!(take(buffer), "hello");
        unsafe { cln_buffer_free(ClnBuffer::EMPTY) };
    }

    #[test]
    fn null_out_pointers_are_rejected() {
        assert_eq!(
            unsafe { cln_environment_probe(std::ptr::null_mut()) },
            CLN_E_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { cln_operation_next(std::ptr::null_mut(), 0, std::ptr::null_mut()) },
            CLN_E_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { cln_operation_cancel(std::ptr::null_mut()) },
            CLN_E_INVALID_ARGUMENT
        );
        unsafe { cln_operation_free(std::ptr::null_mut()) };
    }

    #[test]
    fn panics_become_structured_errors() {
        let mut out = ClnBuffer::EMPTY;
        let status = unsafe { call(&mut out, || panic!("boom")) };
        assert_eq!(status, CLN_E_PANIC);
        let json = take(out);
        assert!(
            json.contains("\"code\":\"internal\"") && json.contains("boom"),
            "{json}"
        );
    }

    #[test]
    fn requests_larger_than_old_limit_are_accepted_but_still_bounded() {
        let mut bytes = b"{}".to_vec();
        bytes.resize(5 * 1024 * 1024, b' ');
        assert!(unsafe { parse_request::<serde_json::Value>(bytes.as_ptr(), bytes.len()) }.is_ok());
        assert!(
            unsafe { parse_request::<serde_json::Value>(bytes.as_ptr(), MAX_REQUEST_BYTES + 1) }
                .is_err()
        );
    }

    #[test]
    fn invalid_requests_return_errors_not_operations() {
        let mut op = std::ptr::null_mut();
        let mut error = ClnBuffer::EMPTY;
        let request = br#"{"unexpected":1}"#;
        let status = unsafe {
            cln_inventory_scan_start(request.as_ptr(), request.len(), &mut op, &mut error)
        };
        assert_eq!(status, CLN_E_INVALID_ARGUMENT);
        assert!(op.is_null());
        assert!(take(error).contains("invalidArgument"));
    }

    #[test]
    fn inventory_scan_streams_batches_then_a_result() {
        let mut op = std::ptr::null_mut();
        let request = br#"{"includeMsix":false,"batchSize":25}"#;
        let status = unsafe {
            cln_inventory_scan_start(
                request.as_ptr(),
                request.len(),
                &mut op,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, CLN_OK);
        let (mut batches, mut result) = (0, None);
        loop {
            let mut out = ClnBuffer::EMPTY;
            match unsafe { cln_operation_next(op, 1000, &mut out) } {
                CLN_MESSAGE => {
                    let json: serde_json::Value = serde_json::from_str(&take(out)).unwrap();
                    match json["type"].as_str() {
                        Some("batch") => {
                            assert!(json["items"].as_array().unwrap().len() <= 25);
                            batches += 1;
                        }
                        Some("result") => result = Some(json["value"].clone()),
                        other => panic!("unexpected message {other:?}"),
                    }
                }
                CLN_TIMEOUT => continue,
                CLN_COMPLETE => break,
                other => panic!("unexpected status {other}"),
            }
        }
        unsafe { cln_operation_free(op) };
        let result = result.expect("result message");
        assert_eq!(result["timings"]["msixMs"], 0);
        assert!(batches > 0 || result["records"] == 0);
    }

    #[test]
    fn cancelled_scans_complete_promptly() {
        let mut op = std::ptr::null_mut();
        let status =
            unsafe { cln_inventory_scan_start(std::ptr::null(), 0, &mut op, std::ptr::null_mut()) };
        assert_eq!(status, CLN_OK);
        unsafe { cln_operation_cancel(op) };
        let started = std::time::Instant::now();
        loop {
            let mut out = ClnBuffer::EMPTY;
            match unsafe { cln_operation_next(op, 100, &mut out) } {
                CLN_MESSAGE => drop(take(out)),
                CLN_TIMEOUT => {}
                CLN_COMPLETE => break,
                other => panic!("unexpected status {other}"),
            }
            assert!(
                started.elapsed().as_secs() < 10,
                "cancellation did not complete"
            );
        }
        unsafe { cln_operation_free(op) };
    }
}
