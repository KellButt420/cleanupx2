use serde::Serialize;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, GetLastError,
};

/// Stable, machine-readable error categories shared with the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    InvalidArgument,
    NotFound,
    AccessDenied,
    /// Input was well-formed but failed a safety rule (for example an
    /// uninstall command that points at a script host).
    Rejected,
    Unsupported,
    Cancelled,
    Win32,
    Internal,
}

/// Structured error that crosses the FFI boundary as JSON.
#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct CoreError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win32_error: Option<u32>,
}

impl CoreError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            win32_error: None,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn rejected(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Rejected, message)
    }

    pub fn win32(context: &str, error: u32) -> Self {
        let code = match error {
            ERROR_ACCESS_DENIED => ErrorCode::AccessDenied,
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => ErrorCode::NotFound,
            _ => ErrorCode::Win32,
        };
        Self {
            code,
            message: format!("{context} failed (Win32 error {error})"),
            win32_error: Some(error),
        }
    }

    /// Captures `GetLastError` immediately; call before any other API call.
    pub fn last_win32(context: &str) -> Self {
        Self::win32(context, unsafe { GetLastError() })
    }

    /// Converts an HRESULT whose facility is Win32 back to the Win32 code.
    pub fn hresult(context: &str, hresult: i32) -> Self {
        let value = hresult as u32;
        if value & 0xFFFF_0000 == 0x8007_0000 {
            return Self::win32(context, value & 0xFFFF);
        }
        Self {
            code: ErrorCode::Win32,
            message: format!("{context} failed (HRESULT 0x{value:08X})"),
            win32_error: None,
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_errors_map_to_stable_codes() {
        assert_eq!(CoreError::win32("x", 5).code, ErrorCode::AccessDenied);
        assert_eq!(CoreError::win32("x", 2).code, ErrorCode::NotFound);
        assert_eq!(CoreError::win32("x", 1618).code, ErrorCode::Win32);
    }

    #[test]
    fn win32_hresults_unwrap_to_win32_codes() {
        let error = CoreError::hresult("x", 0x8007_0005_u32 as i32);
        assert_eq!(error.code, ErrorCode::AccessDenied);
        assert_eq!(error.win32_error, Some(5));
    }
}
