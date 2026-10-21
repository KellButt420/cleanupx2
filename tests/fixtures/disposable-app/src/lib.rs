//! Disposable test application for Cleanup integration tests.
//!
//! It registers itself like a conventionally installed program, with an
//! official uninstaller that behaves the way common installer frameworks do:
//! the registered uninstaller copies itself to `%TEMP%`, relaunches the copy,
//! and exits immediately; the copy then removes the installation.
//!
//! Every stage writes a diagnostics file recording whether it ran with MSIX
//! package identity and elevation, so the harness can verify how processes
//! launched from a packaged app behave. The fixture only ever touches its own
//! namespaced registry key, folders, and temp copies.
//!
//! Harness commands (`cleanup-fixture.exe`):
//!   install --scope user|machine --diag-dir <dir>
//!   status --scope user|machine
//!   purge --scope user|machine
//!
//! Registered uninstaller (`cleanup-disposable-app.exe`, GUI subsystem):
//!   uninstall --scope user|machine --diag-dir <dir>
//!   (no arguments: exit immediately; used as an MSIX entry point)

use std::env;
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::{null, null_mut};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{
    APPMODEL_ERROR_NO_PACKAGE, CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER,
    ERROR_SUCCESS, HANDLE,
};
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, KEY_WOW64_64KEY,
    REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyExW,
    RegOpenKeyExW, RegSetValueExW,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

pub const PUBLISHER: &str = "Cleanup Test Fixtures";
const UNINSTALL_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const STAGE2_PREFIX: &str = "cleanup-disposable-stage2-";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    User,
    Machine,
}

impl Scope {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Scope::User),
            "machine" => Some(Scope::Machine),
            _ => None,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Scope::User => "user",
            Scope::Machine => "machine",
        }
    }

    fn key_name(self) -> &'static str {
        match self {
            Scope::User => "CleanupDisposableTestApp.User",
            Scope::Machine => "CleanupDisposableTestApp.Machine",
        }
    }

    fn root(self) -> HKEY {
        match self {
            Scope::User => HKEY_CURRENT_USER,
            Scope::Machine => HKEY_LOCAL_MACHINE,
        }
    }

    fn install_dir(self) -> PathBuf {
        match self {
            Scope::User => env_path("LOCALAPPDATA").join("CleanupDisposableTestApp"),
            Scope::Machine => env_path("ProgramFiles").join("CleanupDisposableTestApp"),
        }
    }

    fn data_dir(self) -> PathBuf {
        match self {
            Scope::User => env_path("LOCALAPPDATA").join("CleanupDisposableTestApp-Data"),
            Scope::Machine => env_path("ProgramData").join("CleanupDisposableTestApp-Data"),
        }
    }

    /// The registered uninstaller binary for this scope.
    fn uninstaller_name(self) -> &'static str {
        match self {
            Scope::User => "cleanup-disposable-app.exe",
            Scope::Machine => "cleanup-disposable-app-admin.exe",
        }
    }
}

pub fn run() -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return 0;
    };
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let Some(scope) = option("--scope").as_deref().and_then(Scope::parse) else {
        eprintln!("--scope user|machine is required");
        return 2;
    };
    let diag_dir = option("--diag-dir").map(PathBuf::from);

    let result = match command.as_str() {
        "install" => install(scope, diag_dir.as_deref()),
        "uninstall" => uninstall_stage1(scope, diag_dir.as_deref()),
        "uninstall-stage2" => {
            let parent = option("--parent-pid").and_then(|p| p.parse().ok());
            uninstall_stage2(scope, diag_dir.as_deref(), parent)
        }
        "status" => {
            println!("{}", status(scope));
            Ok(())
        }
        "purge" => purge(scope),
        other => Err(format!("unknown command {other:?}")),
    };
    match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn install(scope: Scope, diag_dir: Option<&Path>) -> Result<(), String> {
    let diag_dir = diag_dir.ok_or("--diag-dir is required for install")?;
    let source_dir = env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("no executable directory")?
        .to_path_buf();
    let install_dir = scope.install_dir();
    let data_dir = scope.data_dir();
    fs::create_dir_all(&install_dir).map_err(|e| format!("create {install_dir:?}: {e}"))?;
    fs::create_dir_all(&data_dir).map_err(|e| format!("create {data_dir:?}: {e}"))?;
    fs::create_dir_all(diag_dir).map_err(|e| format!("create {diag_dir:?}: {e}"))?;

    let uninstaller = install_dir.join(scope.uninstaller_name());
    fs::copy(source_dir.join(scope.uninstaller_name()), &uninstaller)
        .map_err(|e| format!("copy uninstaller: {e}"))?;
    fs::write(data_dir.join("settings.ini"), "[fixture]\ncreatedBy=cleanup-disposable-app\n")
        .map_err(|e| format!("write data: {e}"))?;

    let size_kb = fs::metadata(&uninstaller).map(|m| m.len() / 1024).unwrap_or(0) as u32;
    let uninstall_string = format!(
        "\"{}\" uninstall --scope {} --diag-dir \"{}\"",
        uninstaller.display(),
        scope.tag(),
        diag_dir.display()
    );
    let key = RegKey::create(scope)?;
    key.set_string(
        "DisplayName",
        &format!("Cleanup Disposable Test App ({})", scope.tag()),
    )?;
    key.set_string("DisplayVersion", "1.0.0")?;
    key.set_string("Publisher", PUBLISHER)?;
    key.set_string("InstallLocation", &install_dir.display().to_string())?;
    key.set_string("DisplayIcon", &uninstaller.display().to_string())?;
    key.set_string("UninstallString", &uninstall_string)?;
    key.set_string("InstallDate", &today_compact())?;
    key.set_dword("EstimatedSize", size_kb.max(1))?;
    key.set_dword("NoModify", 1)?;
    key.set_dword("NoRepair", 1)?;
    println!("installed {} scope: {}", scope.tag(), install_dir.display());
    Ok(())
}

fn uninstall_stage1(scope: Scope, diag_dir: Option<&Path>) -> Result<(), String> {
    let diag_dir = diag_dir.ok_or("--diag-dir is required")?;
    write_diagnostics(diag_dir, scope, "stage1", None);

    let me = env::current_exe().map_err(|e| e.to_string())?;
    let copy = env::temp_dir().join(format!("{STAGE2_PREFIX}{}.exe", std::process::id()));
    fs::copy(&me, &copy).map_err(|e| format!("copy to temp: {e}"))?;
    Command::new(&copy)
        .args(["uninstall-stage2", "--scope", scope.tag(), "--diag-dir"])
        .arg(diag_dir)
        .args(["--parent-pid", &std::process::id().to_string()])
        .spawn()
        .map_err(|e| format!("relaunch: {e}"))?;
    // Exit immediately, like common uninstallers do.
    Ok(())
}

fn uninstall_stage2(scope: Scope, diag_dir: Option<&Path>, parent: Option<u32>) -> Result<(), String> {
    let diag_dir = diag_dir.ok_or("--diag-dir is required")?;
    if let Some(parent) = parent {
        wait_for_exit(parent);
    }
    // Simulate work so that "first process exited" and "uninstall finished"
    // are clearly different moments.
    sleep(Duration::from_millis(1500));
    write_diagnostics(diag_dir, scope, "stage2", parent);

    let started = std::time::Instant::now();
    let install_dir = scope.install_dir();
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir).map_err(|e| format!("remove {install_dir:?}: {e}"))?;
    }
    let files_ms = started.elapsed().as_millis();
    RegKey::delete(scope)?;
    let registry_ms = started.elapsed().as_millis() - files_ms;
    // The data folder is deliberately left behind as a realistic leftover.
    let _ = fs::write(
        diag_dir.join(format!("{}-stage2-timing.json", scope.tag())),
        serde_json::json!({ "removeFilesMs": files_ms as u64, "removeRegistryMs": registry_ms as u64 }).to_string(),
    );
    write_diagnostics(diag_dir, scope, "stage2-done", parent);
    Ok(())
}

fn status(scope: Scope) -> String {
    let temp_copies = fs::read_dir(env::temp_dir())
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with(STAGE2_PREFIX))
                .count()
        })
        .unwrap_or(0);
    serde_json::json!({
        "scope": scope.tag(),
        "registered": RegKey::exists(scope),
        "installDirExists": scope.install_dir().exists(),
        "dataDirExists": scope.data_dir().exists(),
        "tempCopies": temp_copies,
    })
    .to_string()
}

fn purge(scope: Scope) -> Result<(), String> {
    RegKey::delete(scope)?;
    for dir in [scope.install_dir(), scope.data_dir()] {
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("remove {dir:?}: {e}"))?;
        }
    }
    if let Ok(entries) = fs::read_dir(env::temp_dir()) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(STAGE2_PREFIX) && name.ends_with(".exe") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

fn write_diagnostics(diag_dir: &Path, scope: Scope, stage: &str, parent: Option<u32>) {
    let report = serde_json::json!({
        "stage": stage,
        "scope": scope.tag(),
        "processId": std::process::id(),
        "parentProcessId": parent,
        "executable": env::current_exe().ok().map(|p| p.display().to_string()),
        "packageFullName": package_full_name(),
        "elevated": is_elevated(),
        "timeUnixMs": SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
    });
    let _ = fs::create_dir_all(diag_dir);
    let _ = fs::write(
        diag_dir.join(format!("{}-{stage}.json", scope.tag())),
        report.to_string(),
    );
}

fn wait_for_exit(pid: u32) {
    // SAFETY: plain handle open/wait/close.
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !process.is_null() {
            WaitForSingleObject(process, 10_000);
            CloseHandle(process);
        }
    }
}

fn package_full_name() -> Option<String> {
    let mut len = 0u32;
    // SAFETY: size query, then a correctly sized buffer.
    unsafe {
        match GetCurrentPackageFullName(&mut len, null_mut()) {
            APPMODEL_ERROR_NO_PACKAGE => None,
            ERROR_INSUFFICIENT_BUFFER => {
                let mut buffer = vec![0u16; len as usize];
                (GetCurrentPackageFullName(&mut len, buffer.as_mut_ptr()) == ERROR_SUCCESS)
                    .then(|| String::from_utf16_lossy(&buffer[..(len as usize).saturating_sub(1)]))
            }
            _ => None,
        }
    }
}

fn is_elevated() -> bool {
    let mut token: HANDLE = null_mut();
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut size = 0u32;
    // SAFETY: standard token query on the current process.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut c_void,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        CloseHandle(token);
    }
    elevation.TokenIsElevated != 0
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("{name} is not set")))
}

fn today_compact() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}")
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

struct RegKey(HKEY);

impl RegKey {
    fn path(scope: Scope) -> Vec<u16> {
        wide(&format!(r"{UNINSTALL_PATH}\{}", scope.key_name()))
    }

    fn create(scope: Scope) -> Result<Self, String> {
        let mut key: HKEY = null_mut();
        // SAFETY: valid NUL-terminated path and out pointer.
        let rc = unsafe {
            RegCreateKeyExW(
                scope.root(),
                Self::path(scope).as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_WOW64_64KEY,
                null(),
                &mut key,
                null_mut(),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(format!("RegCreateKeyExW failed ({rc}); machine scope requires elevation"));
        }
        Ok(RegKey(key))
    }

    fn exists(scope: Scope) -> bool {
        let mut key: HKEY = null_mut();
        // SAFETY: valid NUL-terminated path and out pointer.
        let rc = unsafe {
            RegOpenKeyExW(scope.root(), Self::path(scope).as_ptr(), 0, KEY_QUERY_VALUE | KEY_WOW64_64KEY, &mut key)
        };
        if rc == ERROR_SUCCESS {
            // SAFETY: opened above.
            unsafe { RegCloseKey(key) };
        }
        rc == ERROR_SUCCESS
    }

    fn delete(scope: Scope) -> Result<(), String> {
        // SAFETY: valid NUL-terminated path.
        let rc = unsafe { RegDeleteKeyExW(scope.root(), Self::path(scope).as_ptr(), KEY_WOW64_64KEY, 0) };
        match rc {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            rc => Err(format!("RegDeleteKeyExW failed ({rc})")),
        }
    }

    fn set_string(&self, name: &str, value: &str) -> Result<(), String> {
        let data = wide(value);
        // SAFETY: `data` is NUL-terminated UTF-16; size includes the terminator.
        let rc = unsafe {
            RegSetValueExW(self.0, wide(name).as_ptr(), 0, REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32)
        };
        (rc == ERROR_SUCCESS).then_some(()).ok_or(format!("RegSetValueExW {name} failed ({rc})"))
    }

    fn set_dword(&self, name: &str, value: u32) -> Result<(), String> {
        let data = value.to_le_bytes();
        // SAFETY: four bytes for REG_DWORD.
        let rc = unsafe { RegSetValueExW(self.0, wide(name).as_ptr(), 0, REG_DWORD, data.as_ptr(), 4) };
        (rc == ERROR_SUCCESS).then_some(()).ok_or(format!("RegSetValueExW {name} failed ({rc})"))
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: created by RegCreateKeyExW.
        unsafe { RegCloseKey(self.0) };
    }
}
