//! UTF-16 helpers for Win32 calls.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// NUL-terminated UTF-16 copy of `value`.
pub fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// NUL-terminated UTF-16 copy of an OS string, preserving unpaired surrogates.
pub fn os_to_wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

/// Decodes up to the first NUL. Invalid UTF-16 from untrusted sources is
/// replaced rather than rejected.
pub fn from_wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Decodes a NUL-terminated UTF-16 string owned by someone else.
///
/// # Safety
/// `ptr` must be null or point to a readable NUL-terminated UTF-16 string.
pub unsafe fn from_wide_ptr(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    // SAFETY: the caller guarantees a NUL terminator exists.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` elements were just read successfully.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_non_ascii_text() {
        let wide = to_wide("Grüße 日本");
        assert_eq!(wide.last(), Some(&0));
        assert_eq!(from_wide(&wide), "Grüße 日本");
        assert_eq!(unsafe { from_wide_ptr(wide.as_ptr()) }, "Grüße 日本");
    }

    #[test]
    fn stops_at_first_nul_and_tolerates_bad_surrogates() {
        assert_eq!(from_wide(&[0x61, 0, 0x62]), "a");
        assert_eq!(from_wide(&[0xD800, 0x61]), "\u{FFFD}a");
    }
}
