//! MSIX package removal through the package manager.

use std::thread::sleep;
use std::time::Duration;

use cleanup_core::{CoreError, ErrorCode, Result};
use windows::Management::Deployment::PackageManager;
use windows::core::HSTRING;
use windows_future::AsyncStatus;

const POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalOutcome {
    Completed,
    /// Waiting stopped because of cancellation. The removal itself is left to
    /// finish; interrupting package deployment is not attempted.
    Detached,
}

/// Removes the package for the current user. The caller must have joined a
/// WinRT apartment on this thread.
pub fn remove_package(full_name: &str, cancel: &dyn Fn() -> bool) -> Result<RemovalOutcome> {
    let failure =
        |context: &str, error: windows::core::Error| CoreError::hresult(context, error.code().0);
    let manager = PackageManager::new().map_err(|e| failure("PackageManager", e))?;
    let operation = manager
        .RemovePackageAsync(&HSTRING::from(full_name))
        .map_err(|e| failure("RemovePackageAsync", e))?;

    loop {
        match operation
            .Status()
            .map_err(|e| failure("RemovePackageAsync", e))?
        {
            AsyncStatus::Started => {
                if cancel() {
                    return Ok(RemovalOutcome::Detached);
                }
                sleep(POLL);
            }
            AsyncStatus::Completed => return Ok(RemovalOutcome::Completed),
            status => {
                let hresult = operation.ErrorCode().map(|hr| hr.0).unwrap_or(0);
                let detail = operation
                    .GetResults()
                    .and_then(|result| result.ErrorText())
                    .map(|text| text.to_string())
                    .unwrap_or_default();
                let mut error = CoreError::hresult("RemovePackageAsync", hresult);
                if status == AsyncStatus::Canceled {
                    error.code = ErrorCode::Cancelled;
                }
                if !detail.trim().is_empty() {
                    error.message = format!("{} {}", error.message, detail.trim());
                }
                return Err(error);
            }
        }
    }
}
