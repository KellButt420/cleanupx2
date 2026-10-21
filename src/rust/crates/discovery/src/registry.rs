//! Defensive, read-only access to the uninstall registry locations.
//!
//! Values are untrusted: sizes are capped, strings are decoded leniently,
//! and only `REG_EXPAND_SZ` values are environment-expanded.

use std::ptr::{null, null_mut};

use cleanup_core::model::{Hive, RegView};
use cleanup_core::wide::{from_wide, to_wide};
use cleanup_core::{CoreError, Result};
use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS,
};
use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE,
    KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_DWORD, REG_EXPAND_SZ, REG_SAM_FLAGS, REG_SZ,
    REG_VALUE_TYPE, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryInfoKeyW, RegQueryValueExW,
};

pub const UNINSTALL_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

const MAX_VALUE_BYTES: u32 = 64 * 1024;
const MAX_SUBKEYS: usize = 20_000;

/// Raw values of one uninstall entry, before any classification.
#[derive(Debug, Clone, Default)]
pub struct RawUninstallEntry {
    pub key_name: String,
    pub display_name: Option<String>,
    pub display_version: Option<String>,
    pub publisher: Option<String>,
    pub install_location: Option<String>,
    pub install_date: Option<String>,
    pub display_icon: Option<String>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    pub parent_key_name: Option<String>,
    pub release_type: Option<String>,
    pub system_component: Option<u32>,
    pub windows_installer: Option<u32>,
    pub no_remove: Option<u32>,
    pub estimated_size_kb: Option<u32>,
}

/// Human-readable location, e.g. `HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\...\Uninstall\Key`.
pub fn display_path(hive: Hive, view: RegView, key_name: &str) -> String {
    let base = match view {
        RegView::Bits32 => r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        RegView::Bits64 | RegView::Shared => UNINSTALL_PATH,
    };
    format!(r"{}\{base}\{key_name}", hive.display())
}

/// Reads every uninstall entry in one hive/view. Entries that cannot be opened
/// (for example, restrictive ACLs) are skipped rather than failing the scan.
pub fn read_uninstall_entries(
    hive: Hive,
    view: RegView,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<RawUninstallEntry>> {
    let Some(root) = RegKey::open(hive, view, UNINSTALL_PATH)? else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    for name in root.subkey_names()? {
        if cancel() {
            break;
        }
        if let Ok(Some(key)) = root.subkey(&name, view) {
            entries.push(read_entry(&key, name));
        }
    }
    Ok(entries)
}

/// Re-reads a single entry by key name, used to revalidate before acting.
pub fn read_uninstall_entry(
    hive: Hive,
    view: RegView,
    key_name: &str,
) -> Result<Option<RawUninstallEntry>> {
    let path = format!(r"{UNINSTALL_PATH}\{key_name}");
    Ok(RegKey::open(hive, view, &path)?.map(|key| read_entry(&key, key_name.to_owned())))
}

pub fn uninstall_entry_exists(hive: Hive, view: RegView, key_name: &str) -> Result<bool> {
    let path = format!(r"{UNINSTALL_PATH}\{key_name}");
    Ok(RegKey::open(hive, view, &path)?.is_some())
}

fn read_entry(key: &RegKey, key_name: String) -> RawUninstallEntry {
    RawUninstallEntry {
        key_name,
        display_name: key.string("DisplayName"),
        display_version: key.string("DisplayVersion"),
        publisher: key.string("Publisher"),
        install_location: key.string("InstallLocation"),
        install_date: key.string("InstallDate"),
        display_icon: key.string("DisplayIcon"),
        uninstall_string: key.string("UninstallString"),
        quiet_uninstall_string: key.string("QuietUninstallString"),
        parent_key_name: key.string("ParentKeyName"),
        release_type: key.string("ReleaseType"),
        system_component: key.dword("SystemComponent"),
        windows_installer: key.dword("WindowsInstaller"),
        no_remove: key.dword("NoRemove"),
        estimated_size_kb: key.dword("EstimatedSize"),
    }
}

fn root(hive: Hive) -> HKEY {
    match hive {
        Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        Hive::CurrentUser => HKEY_CURRENT_USER,
    }
}

fn view_flags(view: RegView) -> REG_SAM_FLAGS {
    match view {
        RegView::Bits64 => KEY_WOW64_64KEY,
        RegView::Bits32 => KEY_WOW64_32KEY,
        RegView::Shared => 0,
    }
}

/// Owned registry key handle, closed on drop.
struct RegKey(HKEY);

impl RegKey {
    fn open(hive: Hive, view: RegView, path: &str) -> Result<Option<Self>> {
        Self::open_raw(root(hive), path, view)
    }

    fn subkey(&self, name: &str, view: RegView) -> Result<Option<Self>> {
        Self::open_raw(self.0, name, view)
    }

    fn open_raw(parent: HKEY, path: &str, view: RegView) -> Result<Option<Self>> {
        let path = to_wide(path);
        let access = KEY_QUERY_VALUE | KEY_ENUMERATE_SUB_KEYS | view_flags(view);
        let mut key: HKEY = null_mut();
        // SAFETY: `path` is NUL-terminated and `key` is a valid out pointer.
        match unsafe { RegOpenKeyExW(parent, path.as_ptr(), 0, access, &mut key) } {
            ERROR_SUCCESS => Ok(Some(RegKey(key))),
            ERROR_FILE_NOT_FOUND => Ok(None),
            error => Err(CoreError::win32("RegOpenKeyExW", error)),
        }
    }

    fn subkey_names(&self) -> Result<Vec<String>> {
        let mut max_len = 0u32;
        // SAFETY: all optional out pointers are null except `max_len`.
        let rc = unsafe {
            RegQueryInfoKeyW(
                self.0,
                null_mut(),
                null_mut(),
                null(),
                null_mut(),
                &mut max_len,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(CoreError::win32("RegQueryInfoKeyW", rc));
        }

        let mut buffer = vec![0u16; (max_len as usize).max(255) + 1];
        let mut names = Vec::new();
        let mut index = 0u32;
        while names.len() < MAX_SUBKEYS {
            let mut len = buffer.len() as u32;
            // SAFETY: `buffer` holds `len` UTF-16 units.
            let rc = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    buffer.as_mut_ptr(),
                    &mut len,
                    null(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            match rc {
                ERROR_SUCCESS => {
                    // Names that are not valid UTF-16 could not be reopened
                    // reliably from their id, so they are skipped.
                    if let Ok(name) = String::from_utf16(&buffer[..len as usize]) {
                        names.push(name);
                    }
                    index += 1;
                }
                ERROR_NO_MORE_ITEMS => break,
                ERROR_MORE_DATA if buffer.len() < 32_768 => buffer.resize(buffer.len() * 2, 0),
                ERROR_MORE_DATA => index += 1,
                error => return Err(CoreError::win32("RegEnumKeyExW", error)),
            }
        }
        Ok(names)
    }

    fn raw_value(&self, name: &str) -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
        let name = to_wide(name);
        let mut kind: REG_VALUE_TYPE = 0;
        let mut size = 0u32;
        // SAFETY: querying the size only; data pointer is null.
        let rc = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                null(),
                &mut kind,
                null_mut(),
                &mut size,
            )
        };
        if rc != ERROR_SUCCESS || size > MAX_VALUE_BYTES {
            return None;
        }
        // The value can change between calls; retry a few times if it grows.
        for _ in 0..3 {
            let mut data = vec![0u8; size as usize];
            let mut got = size;
            // SAFETY: `data` holds `got` bytes.
            let rc = unsafe {
                RegQueryValueExW(
                    self.0,
                    name.as_ptr(),
                    null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut got,
                )
            };
            match rc {
                ERROR_SUCCESS => {
                    data.truncate(got as usize);
                    return Some((kind, data));
                }
                ERROR_MORE_DATA if got <= MAX_VALUE_BYTES => size = got,
                _ => return None,
            }
        }
        None
    }

    fn string(&self, name: &str) -> Option<String> {
        let (kind, data) = self.raw_value(name)?;
        decode_string(kind, &data)
    }

    fn dword(&self, name: &str) -> Option<u32> {
        let (kind, data) = self.raw_value(name)?;
        match kind {
            REG_DWORD if data.len() >= 4 => {
                Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
            }
            // Some installers store flags as strings.
            REG_SZ => decode_string(kind, &data)?.parse().ok(),
            _ => None,
        }
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: the handle came from RegOpenKeyExW and is closed once.
        unsafe { RegCloseKey(self.0) };
    }
}

fn decode_string(kind: REG_VALUE_TYPE, data: &[u8]) -> Option<String> {
    if kind != REG_SZ && kind != REG_EXPAND_SZ {
        return None;
    }
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let mut value = from_wide(&units);
    if kind == REG_EXPAND_SZ {
        if let Some(expanded) = expand_environment(&value) {
            value = expanded;
        }
    }
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn expand_environment(value: &str) -> Option<String> {
    let source = to_wide(value);
    // SAFETY: size query with a null destination.
    let needed = unsafe { ExpandEnvironmentStringsW(source.as_ptr(), null_mut(), 0) };
    if needed == 0 || needed > 32_768 {
        return None;
    }
    let mut buffer = vec![0u16; needed as usize];
    // SAFETY: `buffer` holds `needed` units.
    let written =
        unsafe { ExpandEnvironmentStringsW(source.as_ptr(), buffer.as_mut_ptr(), needed) };
    (written != 0 && written <= needed).then(|| from_wide(&buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16_bytes(value: &str) -> Vec<u8> {
        value
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    #[test]
    fn decodes_and_trims_strings() {
        assert_eq!(
            decode_string(REG_SZ, &utf16_bytes("  App  ")).as_deref(),
            Some("App")
        );
        assert_eq!(decode_string(REG_SZ, &utf16_bytes("   ")), None);
        assert_eq!(decode_string(REG_DWORD, &[1, 0, 0, 0]), None);
    }

    #[test]
    fn tolerates_odd_lengths_and_missing_terminators() {
        let mut data: Vec<u8> = "abc".encode_utf16().flat_map(u16::to_le_bytes).collect();
        data.push(0x41);
        assert_eq!(decode_string(REG_SZ, &data).as_deref(), Some("abc"));
    }

    #[test]
    fn expands_only_expandable_strings() {
        let raw = utf16_bytes("%SystemRoot%\\x");
        assert_eq!(
            decode_string(REG_SZ, &raw).as_deref(),
            Some("%SystemRoot%\\x")
        );
        let expanded = decode_string(REG_EXPAND_SZ, &raw).unwrap();
        assert!(!expanded.contains('%'), "{expanded}");
    }

    #[test]
    fn display_paths_name_the_wow64_node_for_32_bit_entries() {
        assert_eq!(
            display_path(Hive::LocalMachine, RegView::Bits32, "App"),
            r"HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\App"
        );
    }

    #[test]
    fn reads_the_live_uninstall_key_without_error() {
        // Read-only smoke test against the real registry.
        let entries =
            read_uninstall_entries(Hive::LocalMachine, RegView::Bits64, &|| false).unwrap();
        assert!(entries.iter().all(|e| !e.key_name.is_empty()));
    }
}
