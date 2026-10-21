//! Official uninstall coordination.
//!
//! The frontend sends only an application id. The core re-reads the
//! application's registration from its source, re-validates the uninstall
//! mechanism, runs it, waits for it, and then checks whether the application
//! is still registered. Process exit alone is never reported as removal.

pub mod command;
pub mod launch;
pub mod msix;

use std::fs;
use std::os::windows::fs::MetadataExt;

use cleanup_core::model::AppId;
use cleanup_core::{CoreError, ErrorCode, Result};
use cleanup_discovery::{arp, msi, registry};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, GetDriveTypeW};
use windows_sys::Win32::System::WindowsProgramming::DRIVE_REMOTE;

use crate::command::{ParsedCommand, parse_registered_command};
use crate::launch::{LaunchError, LaunchReport, LaunchRequest, launch_and_wait, system_directory};

const ERROR_INSTALL_USEREXIT: u32 = 1602;
const ERROR_UNKNOWN_PRODUCT: u32 = 1605;
const ERROR_INSTALL_ALREADY_RUNNING: u32 = 1618;
const ERROR_SUCCESS_REBOOT_INITIATED: u32 = 1641;
const ERROR_SUCCESS_REBOOT_REQUIRED: u32 = 3010;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UninstallRequest {
    pub app_id: String,
    #[serde(default)]
    pub allow_elevation: bool,
    /// HWND that should own UAC prompts.
    #[serde(default)]
    pub owner_window: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Validating,
    Running,
    Verifying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mechanism {
    RegisteredUninstaller,
    WindowsInstaller,
    PackageManager,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verification {
    Removed,
    StillPresent,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UninstallStatus {
    /// The uninstaller finished and the registration is gone.
    Removed,
    RemovedRestartRequired,
    /// The uninstaller finished but the application is still registered.
    StillInstalled,
    /// The user declined elevation or cancelled the uninstaller.
    Cancelled,
    Failed,
    /// The uninstall mechanism failed validation and was not run.
    Rejected,
    /// The application was no longer registered when the request arrived.
    NotInstalled,
    /// Completion could not be confirmed.
    Uncertain,
    /// Waiting stopped because of cancellation; the uninstaller may still run.
    Detached,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallReport {
    pub app_id: String,
    pub status: UninstallStatus,
    pub mechanism: Option<Mechanism>,
    pub launch: Option<LaunchReport>,
    pub verification: Verification,
    pub restart_required: bool,
    pub message: String,
    pub error: Option<CoreError>,
}

enum Plan {
    Executable { path: String, arguments: String },
    WindowsInstaller { product_code: String },
    Package { full_name: String },
}

pub fn uninstall(
    request: &UninstallRequest,
    on_stage: &mut dyn FnMut(Stage),
    cancel: &dyn Fn() -> bool,
) -> UninstallReport {
    let mut report = UninstallReport {
        app_id: request.app_id.clone(),
        status: UninstallStatus::Failed,
        mechanism: None,
        launch: None,
        verification: Verification::Unknown,
        restart_required: false,
        message: String::new(),
        error: None,
    };

    on_stage(Stage::Validating);
    let id = match AppId::parse(&request.app_id) {
        Ok(id) => id,
        Err(error) => return finish_with_error(report, UninstallStatus::Rejected, error),
    };
    let plan = match resolve_plan(&id) {
        Ok(Some(plan)) => plan,
        Ok(None) => {
            report.status = UninstallStatus::NotInstalled;
            report.verification = Verification::Removed;
            report.message = "The application is no longer registered. Refresh the list.".into();
            return report;
        }
        Err(error) => {
            let status = if error.code == ErrorCode::Rejected {
                UninstallStatus::Rejected
            } else {
                UninstallStatus::Failed
            };
            return finish_with_error(report, status, error);
        }
    };

    on_stage(Stage::Running);
    let exit_code = match plan {
        Plan::Executable { path, arguments } => {
            report.mechanism = Some(Mechanism::RegisteredUninstaller);
            if let Err(error) = validate_on_disk(&path) {
                let status = if error.code == ErrorCode::Rejected {
                    UninstallStatus::Rejected
                } else {
                    UninstallStatus::Failed
                };
                return finish_with_error(report, status, error);
            }
            match run(&path, &arguments, request, cancel, &mut report) {
                Some(code) => code,
                None => return report,
            }
        }
        Plan::WindowsInstaller { product_code } => {
            report.mechanism = Some(Mechanism::WindowsInstaller);
            let msiexec = format!(r"{}\msiexec.exe", system_directory());
            // Consent is obtained by the caller before starting this operation.
            // Keep Windows Installer's progress UI and suppress automatic reboot.
            let arguments = format!("/x {product_code} /passive /norestart");
            match run(&msiexec, &arguments, request, cancel, &mut report) {
                Some(code) => code,
                None => return report,
            }
        }
        Plan::Package { full_name } => {
            report.mechanism = Some(Mechanism::PackageManager);
            let _apartment = cleanup_discovery::msix::WinRtApartment::enter_mta();
            match msix::remove_package(&full_name, cancel) {
                Ok(msix::RemovalOutcome::Completed) => None,
                Ok(msix::RemovalOutcome::Detached) => {
                    report.status = UninstallStatus::Detached;
                    report.message =
                        "Stopped waiting; Windows may still be removing the package.".into();
                    return report;
                }
                Err(error) => {
                    let status = if error.code == ErrorCode::Cancelled {
                        UninstallStatus::Cancelled
                    } else {
                        UninstallStatus::Failed
                    };
                    report.verification = verify(&id);
                    return finish_with_error(report, status, error);
                }
            }
        }
    };

    on_stage(Stage::Verifying);
    report.verification = verify(&id);
    classify_outcome(&mut report, exit_code);
    report
}

/// Re-reads the registration and decides how it must be removed.
fn resolve_plan(id: &AppId) -> Result<Option<Plan>> {
    match id {
        AppId::Registry {
            hive,
            view,
            key_name,
        } => {
            let Some(entry) = registry::read_uninstall_entry(*hive, *view, key_name)? else {
                return Ok(None);
            };
            if entry.no_remove == Some(1) {
                return Err(CoreError::rejected(
                    "The application's installer does not allow it to be removed.",
                ));
            }
            if let Some(product_code) = arp::msi_product_code(&entry) {
                return Ok(Some(Plan::WindowsInstaller { product_code }));
            }
            let raw = entry.uninstall_string.ok_or_else(|| {
                CoreError::rejected("No uninstall command is registered for this application.")
            })?;
            Ok(Some(match parse_registered_command(&raw)? {
                ParsedCommand::Executable { path, arguments } => {
                    Plan::Executable { path, arguments }
                }
                ParsedCommand::WindowsInstaller { product_code } => {
                    Plan::WindowsInstaller { product_code }
                }
            }))
        }
        AppId::Msi { product_code, .. } => match msi::product_state(product_code) {
            msi::ProductState::Installed | msi::ProductState::Advertised => {
                Ok(Some(Plan::WindowsInstaller {
                    product_code: product_code.clone(),
                }))
            }
            _ => Ok(None),
        },
        AppId::Msix { family_name } => {
            let _apartment = cleanup_discovery::msix::WinRtApartment::enter_mta();
            Ok(cleanup_discovery::msix::find_full_name(family_name)?
                .map(|full_name| Plan::Package { full_name }))
        }
    }
}

/// Checks the executable immediately before launch: it must be an existing
/// regular file on a local drive, not a link.
fn validate_on_disk(path: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        CoreError::new(
            ErrorCode::NotFound,
            format!("The uninstaller {path:?} was not found."),
        )
    })?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(CoreError::rejected(format!(
            "The uninstaller {path:?} is not a regular file."
        )));
    }
    let root = cleanup_core::wide::to_wide(&path[..3]);
    // SAFETY: `root` is a NUL-terminated "X:\" string.
    if unsafe { GetDriveTypeW(root.as_ptr()) } == DRIVE_REMOTE {
        return Err(CoreError::rejected(
            "Uninstallers on network drives are not launched automatically.",
        ));
    }
    Ok(())
}

/// Runs a process-based uninstaller. Returns `None` when the report is final.
fn run(
    executable: &str,
    arguments: &str,
    request: &UninstallRequest,
    cancel: &dyn Fn() -> bool,
    report: &mut UninstallReport,
) -> Option<Option<u32>> {
    let launch = LaunchRequest {
        executable,
        arguments,
        allow_elevation: request.allow_elevation,
        owner_window: request.owner_window as isize,
    };
    match launch_and_wait(&launch, cancel) {
        Ok(launch) if launch.detached => {
            report.status = UninstallStatus::Detached;
            report.message = "Stopped waiting; the uninstaller may still be running.".into();
            report.launch = Some(launch);
            None
        }
        Ok(launch) => {
            let code = launch.exit_code;
            report.launch = Some(launch);
            Some(code)
        }
        Err(LaunchError::ElevationDeclined) => {
            report.status = UninstallStatus::Cancelled;
            report.verification = Verification::StillPresent;
            report.message = "Administrator approval was declined; nothing was changed.".into();
            None
        }
        Err(LaunchError::Failed(error)) => {
            report.status = UninstallStatus::Failed;
            report.message = error.message.clone();
            report.error = Some(error);
            None
        }
    }
}

fn verify(id: &AppId) -> Verification {
    match cleanup_discovery::is_present(id) {
        Ok(true) => Verification::StillPresent,
        Ok(false) => Verification::Removed,
        Err(_) => Verification::Unknown,
    }
}

fn classify_outcome(report: &mut UninstallReport, exit_code: Option<u32>) {
    let is_msi = report.mechanism == Some(Mechanism::WindowsInstaller);
    let restart = is_msi
        && matches!(
            exit_code,
            Some(ERROR_SUCCESS_REBOOT_REQUIRED | ERROR_SUCCESS_REBOOT_INITIATED)
        );
    let tree_tracked = report
        .launch
        .as_ref()
        .is_none_or(|l| l.process_tree_tracked);
    report.restart_required = restart;

    (report.status, report.message) = match report.verification {
        Verification::Removed if restart => (
            UninstallStatus::RemovedRestartRequired,
            "Removed. Restart Windows to finish.".into(),
        ),
        Verification::Removed => (UninstallStatus::Removed, "Removed.".into()),
        Verification::Unknown => (
            UninstallStatus::Uncertain,
            "The uninstaller finished, but removal could not be verified.".into(),
        ),
        Verification::StillPresent => match exit_code {
            Some(ERROR_INSTALL_USEREXIT) if is_msi => {
                (UninstallStatus::Cancelled, "The uninstall was cancelled.".into())
            }
            Some(ERROR_INSTALL_ALREADY_RUNNING) if is_msi => (
                UninstallStatus::Failed,
                "Another installation is in progress. Try again when it finishes.".into(),
            ),
            Some(ERROR_UNKNOWN_PRODUCT) if is_msi => (
                UninstallStatus::Failed,
                "Windows Installer does not recognize this product.".into(),
            ),
            _ if restart => (
                UninstallStatus::Uncertain,
                "Windows Installer requested a restart; check again after restarting.".into(),
            ),
            _ if !tree_tracked => (
                UninstallStatus::Uncertain,
                "The uninstaller's helper processes could not be tracked; it may still be running."
                    .into(),
            ),
            Some(0) => (
                UninstallStatus::StillInstalled,
                "The uninstaller finished, but the application is still installed. It may have been cancelled."
                    .into(),
            ),
            Some(code) => (
                UninstallStatus::Failed,
                format!("The uninstaller exited with code {code} and the application is still installed."),
            ),
            None => (
                UninstallStatus::Uncertain,
                "The uninstaller's exit status is unavailable.".into(),
            ),
        },
    };
}

fn finish_with_error(
    mut report: UninstallReport,
    status: UninstallStatus,
    error: CoreError,
) -> UninstallReport {
    report.status = status;
    report.message = error.message.clone();
    report.error = Some(error);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::LaunchMethod;

    fn report(mechanism: Mechanism, verification: Verification, tracked: bool) -> UninstallReport {
        UninstallReport {
            app_id: "reg:hkcu:shared:Test".into(),
            status: UninstallStatus::Failed,
            mechanism: Some(mechanism),
            launch: Some(LaunchReport {
                method: LaunchMethod::CreateProcess,
                executable: String::new(),
                arguments: String::new(),
                elevation_requested: false,
                process_tree_tracked: tracked,
                processes_observed: 1,
                exit_code: None,
                started_at_unix_ms: 0,
                duration_ms: 0,
                detached: false,
            }),
            verification,
            restart_required: false,
            message: String::new(),
            error: None,
        }
    }

    fn outcome(
        mechanism: Mechanism,
        verification: Verification,
        tracked: bool,
        code: Option<u32>,
    ) -> UninstallStatus {
        let mut r = report(mechanism, verification, tracked);
        classify_outcome(&mut r, code);
        r.status
    }

    #[test]
    fn removal_is_only_reported_when_verified() {
        use Mechanism::*;
        use Verification::*;
        assert_eq!(
            outcome(RegisteredUninstaller, Removed, true, Some(0)),
            UninstallStatus::Removed
        );
        assert_eq!(
            outcome(RegisteredUninstaller, Removed, true, Some(7)),
            UninstallStatus::Removed
        );
        assert_eq!(
            outcome(RegisteredUninstaller, StillPresent, true, Some(0)),
            UninstallStatus::StillInstalled
        );
        assert_eq!(
            outcome(RegisteredUninstaller, StillPresent, true, Some(2)),
            UninstallStatus::Failed
        );
        assert_eq!(
            outcome(RegisteredUninstaller, StillPresent, false, Some(0)),
            UninstallStatus::Uncertain
        );
        assert_eq!(
            outcome(RegisteredUninstaller, Unknown, true, Some(0)),
            UninstallStatus::Uncertain
        );
    }

    #[test]
    fn windows_installer_exit_codes_are_interpreted() {
        use Mechanism::*;
        use Verification::*;
        assert_eq!(
            outcome(WindowsInstaller, Removed, true, Some(3010)),
            UninstallStatus::RemovedRestartRequired
        );
        assert_eq!(
            outcome(WindowsInstaller, StillPresent, true, Some(1602)),
            UninstallStatus::Cancelled
        );
        assert_eq!(
            outcome(WindowsInstaller, StillPresent, true, Some(1618)),
            UninstallStatus::Failed
        );
        assert_eq!(
            outcome(WindowsInstaller, StillPresent, true, Some(3010)),
            UninstallStatus::Uncertain
        );
        // The same codes mean nothing special for arbitrary uninstallers.
        assert_eq!(
            outcome(RegisteredUninstaller, StillPresent, true, Some(1602)),
            UninstallStatus::Failed
        );
    }

    #[test]
    fn invalid_ids_are_rejected_before_anything_runs() {
        let request = UninstallRequest {
            app_id: r"exe:C:\Windows\System32\cmd.exe".into(),
            allow_elevation: false,
            owner_window: 0,
        };
        let mut stages = Vec::new();
        let result = uninstall(&request, &mut |s| stages.push(s), &|| false);
        assert_eq!(result.status, UninstallStatus::Rejected);
        assert_eq!(stages, vec![Stage::Validating]);
        assert!(result.launch.is_none());
    }

    #[test]
    fn unregistered_apps_report_not_installed() {
        let request = UninstallRequest {
            app_id: "reg:hkcu:shared:Cleanup.Tests.DoesNotExist".into(),
            allow_elevation: false,
            owner_window: 0,
        };
        let result = uninstall(&request, &mut |_| {}, &|| false);
        assert_eq!(result.status, UninstallStatus::NotInstalled);
    }

    #[test]
    fn requests_cannot_smuggle_commands() {
        let smuggled = serde_json::from_str::<UninstallRequest>(
            r#"{"appId":"reg:hkcu:shared:App","command":"cmd.exe /c whoami"}"#,
        );
        assert!(smuggled.is_err());
        let minimal =
            serde_json::from_str::<UninstallRequest>(r#"{"appId":"reg:hkcu:shared:App"}"#).unwrap();
        assert!(!minimal.allow_elevation);
    }
}
