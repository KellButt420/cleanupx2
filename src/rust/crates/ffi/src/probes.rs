//! Phase 0 feasibility probes.
//!
//! These exercise platform behavior from inside an installed MSIX package
//! (identity, module locations, known folders, writable app data, file moves).
//! They are diagnostics for the feasibility report, not product features; the
//! organizer and operation journal replace them in later phases.

use std::ffi::c_void;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::mem::size_of;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use cleanup_core::time::unix_millis_now;
use cleanup_core::wide::{from_wide, from_wide_ptr, os_to_wide};
use cleanup_core::{CoreError, ErrorCode, Result};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::{
    APPMODEL_ERROR_NO_PACKAGE, CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, GetLastError,
    HANDLE, HMODULE, WIN32_ERROR,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows_sys::Win32::Storage::Packaging::Appx::{
    GetCurrentPackageFamilyName, GetCurrentPackageFullName,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcessToken,
};
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Downloads, FOLDERID_LocalAppData, KF_FLAG_DEFAULT,
    SHGetKnownFolderPath,
};
use windows_sys::core::{GUID, PWSTR};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    abi_version: u32,
    core_version: &'static str,
    process_id: u32,
    package_full_name: Option<String>,
    package_family_name: Option<String>,
    elevated: Option<bool>,
    executable_path: Option<String>,
    native_library_path: Option<String>,
    known_folders: KnownFolders,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KnownFolders {
    downloads: Option<String>,
    desktop: Option<String>,
    local_app_data: Option<String>,
}

pub fn environment() -> Environment {
    Environment {
        abi_version: crate::CLN_ABI_VERSION,
        core_version: env!("CARGO_PKG_VERSION"),
        // SAFETY: no preconditions.
        process_id: unsafe { GetCurrentProcessId() },
        package_full_name: package_name(GetCurrentPackageFullName),
        package_family_name: package_name(GetCurrentPackageFamilyName),
        elevated: is_elevated(),
        executable_path: module_path(null_mut()),
        native_library_path: own_module().and_then(module_path),
        known_folders: KnownFolders {
            downloads: known_folder(&FOLDERID_Downloads),
            desktop: known_folder(&FOLDERID_Desktop),
            local_app_data: known_folder(&FOLDERID_LocalAppData),
        },
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectoryRequest {
    directory: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistenceResult {
    journal_path: String,
    previous_entries: usize,
    entries: usize,
    recent: Vec<serde_json::Value>,
}

const JOURNAL_FILE: &str = "phase0-persistence-probe.jsonl";

pub fn persistence(request: &DirectoryRequest) -> Result<PersistenceResult> {
    let directory = validate_directory(&request.directory)?;
    let path = directory.join(JOURNAL_FILE);
    let mut records: Vec<serde_json::Value> = match fs::read_to_string(&path) {
        Ok(text) => text
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(io_error("Reading the probe journal", e)),
    };
    let previous_entries = records.len();

    let record = serde_json::json!({
        "launchedAtUnixMs": unix_millis_now(),
        // SAFETY: no preconditions.
        "processId": unsafe { GetCurrentProcessId() },
        "packageFullName": package_name(GetCurrentPackageFullName),
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| io_error("Opening the probe journal", e))?;
    writeln!(file, "{record}").map_err(|e| io_error("Writing the probe journal", e))?;
    file.sync_all()
        .map_err(|e| io_error("Flushing the probe journal", e))?;
    records.push(record);

    let recent = records.iter().rev().take(5).cloned().collect();
    Ok(PersistenceResult {
        journal_path: path.to_string_lossy().into_owned(),
        previous_entries,
        entries: records.len(),
        recent,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMoveResult {
    run_directory: String,
    file_name: String,
    moved: bool,
    overwrite_refused: bool,
    overwrite_error: Option<u32>,
    existing_file_unchanged: bool,
    moved_back: bool,
}

const PROBE_FILE: &str = "probe-ünïcødé-日本.txt";
const PROBE_CONTENT: &str = "cleanup phase 0 probe";

/// Moves fixture files inside a fresh `run-*` subdirectory of `directory`.
/// Nothing outside that subdirectory is read or written.
pub fn file_moves(request: &DirectoryRequest) -> Result<FileMoveResult> {
    let root = validate_directory(&request.directory)?;
    let run = root.join(format!("run-{}", unix_millis_now()));
    let inbox = run.join("inbox");
    let sorted = run.join("sorted");
    for dir in [&inbox, &sorted] {
        fs::create_dir_all(dir).map_err(|e| io_error("Creating probe folders", e))?;
    }
    let write = |path: &Path, content: &str| {
        fs::write(path, content).map_err(|e| io_error("Writing a probe file", e))
    };
    let read = |path: &Path| fs::read_to_string(path).ok();

    // 1. Approved move.
    let source = inbox.join(PROBE_FILE);
    let destination = sorted.join(PROBE_FILE);
    write(&source, PROBE_CONTENT)?;
    let moved = move_without_replacing(&source, &destination).is_ok()
        && !source.exists()
        && read(&destination).as_deref() == Some(PROBE_CONTENT);

    // 2. A move onto an existing name must fail and leave both files intact.
    let incoming = inbox.join("collision.txt");
    let existing = sorted.join("collision.txt");
    write(&incoming, "incoming")?;
    write(&existing, "existing")?;
    let overwrite_error = move_without_replacing(&incoming, &existing).err();
    let existing_file_unchanged = read(&existing).as_deref() == Some("existing")
        && read(&incoming).as_deref() == Some("incoming");

    // 3. Move back (the shape of an undo).
    let moved_back = moved
        && move_without_replacing(&destination, &source).is_ok()
        && read(&source).as_deref() == Some(PROBE_CONTENT)
        && !destination.exists();

    Ok(FileMoveResult {
        run_directory: run.to_string_lossy().into_owned(),
        file_name: PROBE_FILE.to_owned(),
        moved,
        overwrite_refused: overwrite_error.is_some(),
        overwrite_error,
        existing_file_unchanged,
        moved_back,
    })
}

/// `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`. (`std::fs::rename`
/// replaces existing files on Windows, so it is unsuitable for user files.)
fn move_without_replacing(from: &Path, to: &Path) -> std::result::Result<(), u32> {
    let from = os_to_wide(from.as_os_str());
    let to = os_to_wide(to.as_os_str());
    // SAFETY: both paths are NUL-terminated.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        Err(unsafe { GetLastError() })
    } else {
        Ok(())
    }
}

fn validate_directory(directory: &str) -> Result<PathBuf> {
    let path = PathBuf::from(directory);
    if !path.is_absolute() {
        return Err(CoreError::invalid(
            "The directory must be an absolute path.",
        ));
    }
    let metadata = fs::symlink_metadata(&path).map_err(|e| io_error("Opening the directory", e))?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(CoreError::rejected(
            "The path must be a real directory, not a file, link, or junction.",
        ));
    }
    Ok(path)
}

fn io_error(context: &str, error: std::io::Error) -> CoreError {
    match error.raw_os_error() {
        Some(code) => CoreError::win32(context, code as u32),
        None => CoreError::new(ErrorCode::Internal, format!("{context}: {error}")),
    }
}

fn package_name(
    query: unsafe extern "system" fn(*mut u32, PWSTR) -> WIN32_ERROR,
) -> Option<String> {
    let mut len = 0u32;
    // SAFETY: size query with a null buffer.
    match unsafe { query(&mut len, null_mut()) } {
        APPMODEL_ERROR_NO_PACKAGE => return None,
        ERROR_INSUFFICIENT_BUFFER => {}
        _ => return None,
    }
    let mut buffer = vec![0u16; len as usize];
    // SAFETY: `buffer` holds `len` units.
    (unsafe { query(&mut len, buffer.as_mut_ptr()) } == ERROR_SUCCESS).then(|| from_wide(&buffer))
}

fn is_elevated() -> Option<bool> {
    let mut token: HANDLE = null_mut();
    // SAFETY: the pseudo-handle for the current process is always valid.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut size = 0u32;
    // SAFETY: `elevation` matches the TokenElevation information class.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut c_void,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
    };
    // SAFETY: token handle owned here.
    unsafe { CloseHandle(token) };
    (ok != 0).then_some(elevation.TokenIsElevated != 0)
}

fn own_module() -> Option<HMODULE> {
    let mut module: HMODULE = null_mut();
    let address = own_module as *const () as *const u16;
    // SAFETY: looks up the module containing this function without
    // changing its reference count.
    let ok = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            address,
            &mut module,
        )
    };
    (ok != 0).then_some(module)
}

fn module_path(module: HMODULE) -> Option<String> {
    let mut buffer = vec![0u16; 32_768];
    // SAFETY: `buffer` holds its length in units.
    let len = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) };
    (len != 0).then(|| from_wide(&buffer[..len as usize]))
}

fn known_folder(id: &GUID) -> Option<String> {
    let mut path: PWSTR = null_mut();
    // SAFETY: `path` receives a CoTaskMem string that is freed below.
    let hr = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, null_mut(), &mut path) };
    let result = (hr >= 0).then(|| unsafe { from_wide_ptr(path) });
    // SAFETY: freeing null is allowed.
    unsafe { CoTaskMemFree(path as *const c_void) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cleanup-ffi-{name}-{}", unix_millis_now()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn environment_reports_unpackaged_test_process() {
        let env = environment();
        assert!(env.package_full_name.is_none());
        assert!(env.native_library_path.is_some());
        assert!(env.known_folders.downloads.is_some());
    }

    #[test]
    fn persistence_appends_across_calls() {
        let dir = temp_dir("persist");
        let request = DirectoryRequest {
            directory: dir.to_string_lossy().into_owned(),
        };
        assert_eq!(persistence(&request).unwrap().entries, 1);
        let second = persistence(&request).unwrap();
        assert_eq!((second.previous_entries, second.entries), (1, 2));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_moves_never_overwrite() {
        let dir = temp_dir("moves");
        let result = file_moves(&DirectoryRequest {
            directory: dir.to_string_lossy().into_owned(),
        })
        .unwrap();
        assert!(result.moved && result.moved_back);
        assert!(result.overwrite_refused && result.existing_file_unchanged);
        assert_eq!(result.overwrite_error, Some(183)); // ERROR_ALREADY_EXISTS
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_relative_and_missing_directories() {
        for bad in ["relative\\dir", r"C:\definitely\missing\cleanup"] {
            assert!(validate_directory(bad).is_err(), "{bad}");
        }
    }
}
