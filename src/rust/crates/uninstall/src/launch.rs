//! Launching a validated uninstaller and waiting for its whole process tree.
//!
//! Many uninstallers copy themselves to a temporary folder, relaunch that
//! copy, and exit immediately. Waiting on the first process alone would report
//! completion long before anything is removed, so the launched process is
//! placed in a job object and completion means "no process in the job is still
//! running".
//!
//! Inside an MSIX package, processes started from executables outside the
//! package run outside the package's desktop-app runtime environment by
//! default (`PROCESS_CREATION_DESKTOP_APP_BREAKAWAY_ENABLE_PROCESS_TREE` is the
//! documented default), so uninstallers see the real registry and file system.
//! The Phase 0 feasibility run verifies this on the installed package.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::Instant;

use cleanup_core::time::unix_millis_now;
use cleanup_core::wide::{from_wide, to_wide};
use cleanup_core::{CoreError, ErrorCode};
use serde::Serialize;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_CANCELLED, ERROR_ELEVATION_REQUIRED, FALSE, GetLastError, HANDLE, HWND,
    INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::System::IO::{
    CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_ASSOCIATE_COMPLETION_PORT,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectAssociateCompletionPortInformation,
    JobObjectBasicAccountingInformation, QueryInformationJobObject, SetInformationJobObject,
};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::SystemServices::{
    JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO, JOB_OBJECT_MSG_NEW_PROCESS,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess,
    PROCESS_INFORMATION, ResumeThread, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const POLL_MS: u32 = 200;

pub struct LaunchRequest<'a> {
    /// Validated, drive-absolute path to the uninstaller executable.
    pub executable: &'a str,
    pub arguments: &'a str,
    /// Whether a UAC prompt may be shown if the uninstaller requires it.
    pub allow_elevation: bool,
    /// Window that owns the UAC prompt, or 0.
    pub owner_window: isize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchMethod {
    CreateProcess,
    ShellExecuteElevated,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchReport {
    pub method: LaunchMethod,
    pub executable: String,
    pub arguments: String,
    pub elevation_requested: bool,
    /// True when the launched process was placed in a job object, so
    /// completion covers every process it started.
    pub process_tree_tracked: bool,
    pub processes_observed: u32,
    pub exit_code: Option<u32>,
    pub started_at_unix_ms: u64,
    pub duration_ms: u64,
    /// True when waiting stopped because of cancellation; the uninstaller
    /// itself is never terminated.
    pub detached: bool,
}

#[derive(Debug)]
pub enum LaunchError {
    ElevationDeclined,
    Failed(CoreError),
}

pub fn launch_and_wait(
    request: &LaunchRequest,
    cancel: &dyn Fn() -> bool,
) -> Result<LaunchReport, LaunchError> {
    let started = Instant::now();
    let tracker = JobTracker::new().map_err(LaunchError::Failed)?;
    let mut report = LaunchReport {
        method: LaunchMethod::CreateProcess,
        executable: request.executable.to_owned(),
        arguments: request.arguments.to_owned(),
        elevation_requested: false,
        process_tree_tracked: false,
        processes_observed: 0,
        exit_code: None,
        started_at_unix_ms: unix_millis_now(),
        duration_ms: 0,
        detached: false,
    };

    let process = match create_suspended(request.executable, request.arguments) {
        Ok(info) => {
            let process = Handle(info.hProcess);
            let thread = Handle(info.hThread);
            report.process_tree_tracked = tracker.assign(process.0);
            // SAFETY: `thread` is the primary thread of the suspended process.
            if unsafe { ResumeThread(thread.0) } == u32::MAX {
                let error = CoreError::last_win32("ResumeThread");
                // SAFETY: the process never ran; terminating it is safe.
                unsafe { TerminateProcess(process.0, 1) };
                return Err(LaunchError::Failed(error));
            }
            process
        }
        Err(ERROR_ELEVATION_REQUIRED) if request.allow_elevation => {
            report.method = LaunchMethod::ShellExecuteElevated;
            report.elevation_requested = true;
            let process = shell_execute_elevated(request)?;
            // Usually denied for an elevated target; completion then falls
            // back to the primary process.
            report.process_tree_tracked = tracker.assign(process.0);
            process
        }
        Err(ERROR_ELEVATION_REQUIRED) => {
            return Err(LaunchError::Failed(CoreError::new(
                ErrorCode::AccessDenied,
                "The uninstaller requires administrator approval.",
            )));
        }
        Err(error) => {
            return Err(LaunchError::Failed(CoreError::win32(
                "CreateProcessW",
                error,
            )));
        }
    };

    let completed = if report.process_tree_tracked {
        tracker.wait_for_tree(cancel, &mut report.processes_observed)
    } else {
        report.processes_observed = 1;
        wait_for_process(process.0, cancel)
    };
    report.detached = !completed;
    if completed {
        let mut code = 0u32;
        // SAFETY: `process` is a valid process handle.
        if unsafe { GetExitCodeProcess(process.0, &mut code) } != 0 {
            report.exit_code = Some(code);
        }
    }
    report.duration_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}

pub fn system_directory() -> String {
    let mut buffer = [0u16; 260];
    // SAFETY: `buffer` holds 260 units.
    let len = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if len == 0 || len as usize >= buffer.len() {
        return r"C:\Windows\System32".to_owned();
    }
    from_wide(&buffer[..len as usize])
}

fn create_suspended(executable: &str, arguments: &str) -> Result<PROCESS_INFORMATION, u32> {
    let application = to_wide(executable);
    // The path has been validated to contain no quotes.
    let mut command_line = to_wide(&if arguments.is_empty() {
        format!("\"{executable}\"")
    } else {
        format!("\"{executable}\" {arguments}")
    });
    // Never start uninstallers in their own directory: an open working
    // directory handle prevents them from deleting it.
    let directory = to_wide(&system_directory());
    // SAFETY: zero-initialized plain-old-data structures.
    let mut startup: STARTUPINFOW = unsafe { zeroed() };
    startup.cb = size_of::<STARTUPINFOW>() as u32;
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    // SAFETY: all strings are NUL-terminated and outlive the call; an explicit
    // application name disables the executable search.
    let ok = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            FALSE,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            null(),
            directory.as_ptr(),
            &startup,
            &mut info,
        )
    };
    if ok == 0 {
        Err(unsafe { GetLastError() })
    } else {
        Ok(info)
    }
}

fn shell_execute_elevated(request: &LaunchRequest) -> Result<Handle, LaunchError> {
    let _apartment = ComApartment::enter_sta();
    let verb = to_wide("runas");
    let file = to_wide(request.executable);
    let parameters = to_wide(request.arguments);
    let directory = to_wide(&system_directory());
    // SAFETY: zero-initialized plain-old-data structure.
    let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
    info.hwnd = request.owner_window as HWND;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.lpDirectory = directory.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    // SAFETY: every pointer in `info` refers to a live NUL-terminated buffer.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = unsafe { GetLastError() };
        return Err(if error == ERROR_CANCELLED {
            LaunchError::ElevationDeclined
        } else {
            LaunchError::Failed(CoreError::win32("ShellExecuteExW", error))
        });
    }
    let process = Handle(info.hProcess);
    if !process.is_valid() {
        return Err(LaunchError::Failed(CoreError::new(
            ErrorCode::Internal,
            "The elevated uninstaller did not return a process handle.",
        )));
    }
    Ok(process)
}

fn wait_for_process(process: HANDLE, cancel: &dyn Fn() -> bool) -> bool {
    loop {
        // SAFETY: `process` is a valid handle with SYNCHRONIZE access.
        if unsafe { WaitForSingleObject(process, POLL_MS) } == WAIT_OBJECT_0 {
            return true;
        }
        if cancel() {
            return false;
        }
    }
}

struct Handle(HANDLE);

impl Handle {
    fn is_valid(&self) -> bool {
        !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.is_valid() {
            // SAFETY: owned handle, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Job object whose notifications arrive on a private completion port.
/// The job is not configured to kill its processes when closed.
struct JobTracker {
    job: Handle,
    port: Handle,
}

impl JobTracker {
    fn new() -> Result<Self, CoreError> {
        // SAFETY: anonymous job with default security.
        let job = Handle(unsafe { CreateJobObjectW(null(), null()) });
        if !job.is_valid() {
            return Err(CoreError::last_win32("CreateJobObjectW"));
        }
        // SAFETY: creates a new, unassociated completion port.
        let port =
            Handle(unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, null_mut(), 0, 1) });
        if !port.is_valid() {
            return Err(CoreError::last_win32("CreateIoCompletionPort"));
        }
        let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
            CompletionKey: job.0,
            CompletionPort: port.0,
        };
        // SAFETY: `association` matches the information class.
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectAssociateCompletionPortInformation,
                &association as *const _ as *const c_void,
                size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
            )
        };
        if ok == 0 {
            return Err(CoreError::last_win32("SetInformationJobObject"));
        }
        Ok(Self { job, port })
    }

    fn assign(&self, process: HANDLE) -> bool {
        // SAFETY: both handles are valid.
        unsafe { AssignProcessToJobObject(self.job.0, process) != 0 }
    }

    /// Returns `true` once no process in the job is running, or `false` if
    /// waiting was cancelled first.
    fn wait_for_tree(&self, cancel: &dyn Fn() -> bool, processes: &mut u32) -> bool {
        loop {
            let mut message = 0u32;
            let mut key = 0usize;
            let mut overlapped: *mut OVERLAPPED = null_mut();
            // SAFETY: all out pointers are valid locals.
            let ok = unsafe {
                GetQueuedCompletionStatus(
                    self.port.0,
                    &mut message,
                    &mut key,
                    &mut overlapped,
                    POLL_MS,
                )
            };
            if ok != 0 && key == self.job.0 as usize {
                match message {
                    JOB_OBJECT_MSG_NEW_PROCESS => *processes += 1,
                    JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO => return true,
                    _ => {}
                }
            }
            if cancel() {
                return false;
            }
            // Completion-port messages are hints, not a guaranteed delivery contract.
            // Query the job itself so a dropped ACTIVE_PROCESS_ZERO cannot hang the queue.
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
            let queried = unsafe {
                QueryInformationJobObject(
                    self.job.0,
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as *mut c_void,
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    null_mut(),
                )
            };
            if queried == 0 {
                return false;
            } // detach; never terminate an uninstaller on an observation error
            if accounting.ActiveProcesses == 0 {
                return true;
            }
        }
    }
}

/// Single-threaded COM apartment for `ShellExecuteEx`, as its documentation
/// recommends.
struct ComApartment(bool);

impl ComApartment {
    fn enter_sta() -> Self {
        // SAFETY: balanced in Drop when it succeeds (S_OK or S_FALSE).
        let hr = unsafe {
            CoInitializeEx(
                null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        Self(hr >= 0)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: matches the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_job_finishes_without_any_completion_message() {
        let tracker = JobTracker::new().unwrap();
        let start = Instant::now();
        assert!(tracker.wait_for_tree(&|| start.elapsed().as_secs() >= 1, &mut 0));
    }

    #[test]
    fn waits_for_the_whole_process_tree() {
        // cmd.exe starts a child (ping) and both must exit before completion.
        let cmd = format!(r"{}\cmd.exe", system_directory());
        let request = LaunchRequest {
            executable: &cmd,
            arguments: "/d /c ping -n 2 127.0.0.1 >nul",
            allow_elevation: false,
            owner_window: 0,
        };
        let report = launch_and_wait(&request, &|| false).unwrap();
        assert!(report.process_tree_tracked);
        assert!(report.processes_observed >= 2, "{report:?}");
        assert_eq!(report.exit_code, Some(0));
        assert!(!report.detached);
    }

    #[test]
    fn cancellation_detaches_without_killing() {
        let ping = format!(r"{}\PING.EXE", system_directory());
        let request = LaunchRequest {
            executable: &ping,
            arguments: "-n 3 127.0.0.1",
            allow_elevation: false,
            owner_window: 0,
        };
        let report = launch_and_wait(&request, &|| true).unwrap();
        assert!(report.detached);
        assert_eq!(report.exit_code, None);
    }

    #[test]
    fn missing_executables_fail_cleanly() {
        let request = LaunchRequest {
            executable: r"C:\definitely\not\here.exe",
            arguments: "",
            allow_elevation: false,
            owner_window: 0,
        };
        match launch_and_wait(&request, &|| false) {
            Err(LaunchError::Failed(error)) => assert_eq!(error.code, ErrorCode::NotFound),
            other => panic!("unexpected {other:?}"),
        }
    }
}
