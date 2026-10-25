//! Registry access for leftover review: listing subkeys, exporting a key to a
//! `.reg` backup, and deleting a key tree.

use std::ptr::{null, null_mut};
#[cfg(test)]
use windows_sys::Win32::System::Registry::KEY_QUERY_VALUE;

use cleanup_core::model::{Hive, RegView};
use cleanup_core::wide::to_wide;
use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_READ, KEY_WOW64_32KEY,
    KEY_WOW64_64KEY, REG_BINARY, REG_DWORD, REG_EXPAND_SZ, REG_LINK, REG_MULTI_SZ,
    REG_OPTION_OPEN_LINK, REG_QWORD, REG_SAM_FLAGS, REG_SZ, RegCloseKey, RegEnumKeyExW,
    RegEnumValueW, RegOpenKeyExW,
};

pub fn root(hive: Hive) -> HKEY {
    match hive {
        Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        Hive::CurrentUser => HKEY_CURRENT_USER,
    }
}

pub fn view_flags(view: RegView) -> REG_SAM_FLAGS {
    match view {
        RegView::Bits64 => KEY_WOW64_64KEY,
        RegView::Bits32 => KEY_WOW64_32KEY,
        RegView::Shared => 0,
    }
}

pub fn display_path(hive: Hive, view: RegView, subkey: &str) -> String {
    let subkey = match (view, subkey.split_once('\\')) {
        (RegView::Bits32, Some((first, rest))) if first.eq_ignore_ascii_case("SOFTWARE") => {
            format!(r"SOFTWARE\WOW6432Node\{rest}")
        }
        _ => subkey.to_owned(),
    };
    format!(r"{}\{subkey}", hive.display())
}

pub struct Key(HKEY);

impl Key {
    pub fn open(hive: Hive, view: RegView, subkey: &str, access: REG_SAM_FLAGS) -> Option<Key> {
        let path = to_wide(subkey);
        let mut key: HKEY = null_mut();
        // SAFETY: valid NUL-terminated path and out pointer.
        let rc = unsafe {
            RegOpenKeyExW(
                root(hive),
                path.as_ptr(),
                REG_OPTION_OPEN_LINK,
                access | view_flags(view),
                &mut key,
            )
        };
        (rc == ERROR_SUCCESS).then_some(Key(key))
    }

    fn open_child(&self, name: &str, view: RegView) -> Option<Key> {
        let name = to_wide(name);
        let mut key: HKEY = null_mut();
        // SAFETY: valid NUL-terminated name and out pointer.
        let rc = unsafe {
            RegOpenKeyExW(
                self.0,
                name.as_ptr(),
                REG_OPTION_OPEN_LINK,
                KEY_READ | view_flags(view),
                &mut key,
            )
        };
        (rc == ERROR_SUCCESS).then_some(Key(key))
    }

    pub fn subkeys(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut buffer = vec![0u16; 512];
        for index in 0..10_000u32 {
            let mut len = buffer.len() as u32;
            // SAFETY: `buffer` holds `len` units.
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
                    if let Ok(name) = String::from_utf16(&buffer[..len as usize]) {
                        names.push(name);
                    }
                }
                ERROR_NO_MORE_ITEMS => break,
                _ => break,
            }
        }
        names
    }

    /// (name, type, data) for every value. Returns `None` if any value could
    /// not be read completely, so an incomplete backup is never produced.
    fn values(&self) -> Option<Vec<(String, u32, Vec<u8>)>> {
        let mut values = Vec::new();
        let mut name = vec![0u16; 16_384];
        let mut data = vec![0u8; 64 * 1024];
        let mut index = 0u32;
        let mut total_bytes = 0usize;
        loop {
            let mut name_len = name.len() as u32;
            let mut data_len = data.len() as u32;
            let mut kind = 0u32;
            // SAFETY: buffers sized as declared.
            let rc = unsafe {
                RegEnumValueW(
                    self.0,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut data_len,
                )
            };
            match rc {
                ERROR_SUCCESS => {
                    total_bytes += data_len as usize + name_len as usize * 2;
                    if total_bytes > 2 * 1024 * 1024 || values.len() >= 10_000 {
                        return None;
                    }
                    values.push((
                        String::from_utf16_lossy(&name[..name_len as usize]),
                        kind,
                        data[..data_len as usize].to_vec(),
                    ));
                    index += 1;
                }
                ERROR_MORE_DATA
                    if data_len as usize > data.len() && data_len <= 16 * 1024 * 1024 =>
                {
                    data.resize(data_len as usize, 0);
                }
                ERROR_NO_MORE_ITEMS => return Some(values),
                _ => return None,
            }
        }
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: opened by RegOpenKeyExW.
        unsafe { RegCloseKey(self.0) };
    }
}

#[cfg(test)]
pub fn exists(hive: Hive, view: RegView, subkey: &str) -> bool {
    Key::open(hive, view, subkey, KEY_QUERY_VALUE).is_some()
}

pub fn subkeys(hive: Hive, view: RegView, subkey: &str) -> Vec<String> {
    Key::open(hive, view, subkey, KEY_ENUMERATE_SUB_KEYS)
        .map(|k| k.subkeys())
        .unwrap_or_default()
}

/// Exports a key and its subkeys in `.reg` format (Registry Editor 5.00).
pub fn export(hive: Hive, view: RegView, subkey: &str) -> Option<String> {
    let key = Key::open(hive, view, subkey, KEY_READ)?;
    let mut text = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    export_key(&key, &display_path(hive, view, subkey), view, &mut text, 0)?;
    Some(text)
}

fn export_key(key: &Key, path: &str, view: RegView, out: &mut String, depth: u32) -> Option<()> {
    if depth > 32 || out.len() > 8 * 1024 * 1024 {
        return None;
    }
    out.push_str(&format!("[{path}]\r\n"));
    for (name, kind, data) in key.values()? {
        if kind == REG_LINK {
            return None;
        }
        let name = if name.is_empty() {
            "@".to_owned()
        } else {
            format!("\"{}\"", escape(&name))
        };
        let value = match kind {
            REG_SZ => format!("\"{}\"", escape(&utf16_string(&data))),
            REG_DWORD if data.len() >= 4 => format!(
                "dword:{:08x}",
                u32::from_le_bytes([data[0], data[1], data[2], data[3]])
            ),
            REG_EXPAND_SZ => format!("hex(2):{}", hex(&data)),
            REG_MULTI_SZ => format!("hex(7):{}", hex(&data)),
            REG_QWORD => format!("hex(b):{}", hex(&data)),
            REG_BINARY => format!("hex:{}", hex(&data)),
            other => format!("hex({other:x}):{}", hex(&data)),
        };
        out.push_str(&format!("{name}={value}\r\n"));
    }
    out.push_str("\r\n");
    for child in key.subkeys() {
        let child_key = key.open_child(&child, view)?;
        export_key(
            &child_key,
            &format!(r"{path}\{child}"),
            view,
            out,
            depth + 1,
        )?;
    }
    Some(())
}

fn utf16_string(data: &[u8]) -> String {
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    cleanup_core::wide::from_wide(&units)
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Deletes the key and everything below it.
pub fn delete_tree(hive: Hive, view: RegView, subkey: &str) -> Result<(), u32> {
    let key = validate_real_key(hive, subkey)?;
    let flag = if view == RegView::Bits32 {
        "/reg:32"
    } else {
        "/reg:64"
    };
    if !reg_command(&["delete", &key, "/f", flag])? {
        return Err(5);
    }
    if reg_command(&["query", &key, flag])? {
        return Err(5);
    }
    Ok(())
}

pub fn backup_real(
    hive: Hive,
    view: RegView,
    subkey: &str,
    file: &std::path::Path,
) -> Result<(), u32> {
    let key = validate_real_key(hive, subkey)?;
    let flag = if view == RegView::Bits32 {
        "/reg:32"
    } else {
        "/reg:64"
    };
    if file.exists() {
        return Err(183);
    }
    if !reg_command(&["export", &key, &file.to_string_lossy(), flag])? {
        return Err(5);
    }
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(file)
        .and_then(|file| file.sync_all())
        .map_err(|_| 5u32)
}

fn validate_real_key(hive: Hive, subkey: &str) -> Result<String, u32> {
    let parts: Vec<&str> = subkey.split('\\').collect();
    if parts.len() < 2
        || !parts[0].eq_ignore_ascii_case("SOFTWARE")
        || parts.iter().any(|part| part.is_empty())
        || subkey.contains(['\0', '"', '\r', '\n'])
        || ["Microsoft", "Classes", "Policies", "WOW6432Node", "Clients"]
            .iter()
            .any(|name| parts[1].eq_ignore_ascii_case(name))
    {
        return Err(87);
    }
    Ok(format!("{}\\{subkey}", hive.display()))
}

fn reg_command(arguments: &[&str]) -> Result<bool, u32> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let windows =
        cleanup_core::paths::known_folder(cleanup_core::paths::KnownFolder::Windows).ok_or(3u32)?;
    // External Windows executable, fixed command verbs, current token, no shell.
    // Its registry access is outside the packaged process's virtualized view.
    Command::new(windows.join("System32\\reg.exe"))
        .args(arguments)
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|error| error.raw_os_error().unwrap_or(5) as u32)
}
