//! Path safety rules shared by the organizer and the leftover scanner.
//!
//! Comparisons are case-insensitive and component-based, so `C:\Program
//! Files2` is not considered inside `C:\Program Files`.

use std::ffi::c_void;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf, Prefix};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    MOVEFILE_COPY_ALLOWED, MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_EXISTING,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_LocalAppData,
    FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Profile, FOLDERID_ProgramData,
    FOLDERID_ProgramFiles, FOLDERID_ProgramFilesCommon, FOLDERID_ProgramFilesX86,
    FOLDERID_RoamingAppData, FOLDERID_Videos, FOLDERID_Windows, KF_FLAG_DEFAULT,
    SHGetKnownFolderPath,
};
use windows_sys::core::{GUID, PWSTR};

use crate::wide::{from_wide_ptr, os_to_wide};

/// Folders resolved through the known-folder API, never assumed to live
/// under `C:\Users`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownFolder {
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    Profile,
    RoamingAppData,
    LocalAppData,
    ProgramData,
    ProgramFiles,
    ProgramFilesX86,
    ProgramFilesCommon,
    Windows,
}

pub fn known_folder(folder: KnownFolder) -> Option<PathBuf> {
    let id: &GUID = match folder {
        KnownFolder::Desktop => &FOLDERID_Desktop,
        KnownFolder::Documents => &FOLDERID_Documents,
        KnownFolder::Downloads => &FOLDERID_Downloads,
        KnownFolder::Music => &FOLDERID_Music,
        KnownFolder::Pictures => &FOLDERID_Pictures,
        KnownFolder::Videos => &FOLDERID_Videos,
        KnownFolder::Profile => &FOLDERID_Profile,
        KnownFolder::RoamingAppData => &FOLDERID_RoamingAppData,
        KnownFolder::LocalAppData => &FOLDERID_LocalAppData,
        KnownFolder::ProgramData => &FOLDERID_ProgramData,
        KnownFolder::ProgramFiles => &FOLDERID_ProgramFiles,
        KnownFolder::ProgramFilesX86 => &FOLDERID_ProgramFilesX86,
        KnownFolder::ProgramFilesCommon => &FOLDERID_ProgramFilesCommon,
        KnownFolder::Windows => &FOLDERID_Windows,
    };
    let mut path: PWSTR = null_mut();
    // SAFETY: `path` receives a CoTaskMem string that is freed below.
    let hr = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, null_mut(), &mut path) };
    let result = (hr >= 0).then(|| PathBuf::from(unsafe { from_wide_ptr(path) }));
    // SAFETY: freeing null is allowed.
    unsafe { CoTaskMemFree(path as *const c_void) };
    result.filter(|p| !p.as_os_str().is_empty())
}

/// Lower-cased, separator-normalized components without the verbatim prefix.
fn components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Prefix(prefix) => Some(match prefix.kind() {
                Prefix::VerbatimDisk(letter) | Prefix::Disk(letter) => {
                    format!("{}:", (letter as char).to_ascii_lowercase())
                }
                _ => prefix.as_os_str().to_string_lossy().to_lowercase(),
            }),
            Component::RootDir | Component::CurDir => None,
            Component::ParentDir => Some("..".into()),
            Component::Normal(part) => Some(part.to_string_lossy().to_lowercase()),
        })
        .collect()
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    components(a) == components(b)
}

/// True when `child` is `parent` or lies beneath it.
pub fn is_within(child: &Path, parent: &Path) -> bool {
    let child = components(child);
    let parent = components(parent);
    child.len() >= parent.len() && child[..parent.len()] == parent[..]
}

/// Absolute, local, drive-based paths without `..` components.
pub fn is_plain_absolute(path: &Path) -> bool {
    let text = path.as_os_str().to_string_lossy();
    if text.contains(['\0', '<', '>', '"', '|', '*', '?']) && !text.starts_with(r"\\?\") {
        return false;
    }
    let mut parts = path.components();
    let local_drive = matches!(
        parts.next(),
        Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    );
    local_drive
        && path.has_root()
        && !path.components().any(|c| match c {
            Component::ParentDir => true,
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                name.contains(['\0', ':', '<', '>', '"', '|', '*', '?'])
                    || name.ends_with([' ', '.'])
            }
            _ => false,
        })
}

/// Holds existing directory ancestors open without delete sharing. This prevents
/// their replacement while a validated operation uses paths beneath them.
pub struct DirectoryGuard(Vec<HANDLE>);

impl DirectoryGuard {
    pub fn parents(path: &Path) -> Result<Self, String> {
        let mut guard = Self(Vec::new());
        let parent = path.parent().ok_or("Invalid parent directory.")?;
        for directory in parent.ancestors().collect::<Vec<_>>().into_iter().rev() {
            match fs::symlink_metadata(directory) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(_) => return Err("A parent folder cannot be inspected.".into()),
                Ok(m) if !m.is_dir() || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 => {
                    return Err(
                        "A parent folder is a link or junction; it was left unchanged.".into(),
                    );
                }
                _ => {}
            }
            let wide = os_to_wide(verbatim(directory).as_os_str());
            // SAFETY: directory is an absolute NUL-terminated path. No delete
            // sharing keeps it from being replaced while the guard is alive.
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    FILE_READ_ATTRIBUTES,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                    std::ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(describe_file_error(unsafe { GetLastError() }));
            }
            guard.0.push(handle);
            // Recheck after opening, while replacement is locked out.
            if is_reparse_point(directory) {
                return Err("A parent folder became a link.".into());
            }
        }
        Ok(guard)
    }
}

impl Drop for DirectoryGuard {
    fn drop(&mut self) {
        for handle in &self.0 {
            unsafe { CloseHandle(*handle) };
        }
    }
}

pub fn is_reparse_point(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

pub fn is_drive_root(path: &Path) -> bool {
    components(path).len() <= 1
}

/// Folders that must never be organized or deleted as a whole: system and
/// program roots, per-user data roots, and the user's own library folders.
pub fn protected_roots() -> Vec<PathBuf> {
    use KnownFolder::*;
    [
        Windows,
        ProgramFiles,
        ProgramFilesX86,
        ProgramFilesCommon,
        ProgramData,
        Profile,
        RoamingAppData,
        LocalAppData,
        Desktop,
        Documents,
        Downloads,
        Music,
        Pictures,
        Videos,
    ]
    .into_iter()
    .filter_map(known_folder)
    .collect()
}

/// System locations whose contents must never be organized or removed.
pub fn system_roots() -> Vec<PathBuf> {
    use KnownFolder::*;
    [Windows, ProgramFiles, ProgramFilesX86, ProgramFilesCommon]
        .into_iter()
        .filter_map(known_folder)
        .collect()
}

/// Adds the `\\?\` prefix so Win32 calls accept paths beyond `MAX_PATH`.
pub fn verbatim(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if text.starts_with(r"\\?\") || !is_plain_absolute(path) {
        path.to_path_buf()
    } else {
        PathBuf::from(format!(r"\\?\{}", text.replace('/', "\\")))
    }
}

/// Moves a file without ever replacing an existing destination. Moves across
/// volumes are performed as copy-then-delete by the system.
pub fn move_no_replace(from: &Path, to: &Path) -> Result<(), u32> {
    let original_path = from.to_path_buf();
    let from = os_to_wide(verbatim(from).as_os_str());
    let to = os_to_wide(verbatim(to).as_os_str());
    // SAFETY: both paths are NUL-terminated.
    let ok = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_COPY_ALLOWED | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(unsafe { GetLastError() })
    } else if fs::symlink_metadata(&original_path).is_ok() {
        // Windows may report a cross-volume copy as successful even when the
        // source could not be deleted. Preserve both files and report the issue.
        Err(0xE0010001)
    } else {
        Ok(())
    }
}

/// Plain-language explanation for common file-operation errors.
pub fn describe_file_error(code: u32) -> String {
    match code {
        0xE0010001 => "A copy exists at the destination, but the original remains. Inspect both paths before retrying.".into(),
        2 | 3 => "The file no longer exists.".into(),
        5 => "Windows denied access to the file.".into(),
        32 | 33 => "The file is open in another app.".into(),
        80 | 183 => "A file with that name already exists.".into(),
        112 => "There isn't enough free space.".into(),
        206 => "The path is too long.".into(),
        _ => format!("Windows reported error {code}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containment_is_component_based_and_case_insensitive() {
        assert!(is_within(
            Path::new(r"C:\Program Files\App"),
            Path::new(r"c:\program files")
        ));
        assert!(is_within(
            Path::new(r"\\?\C:\Data\x"),
            Path::new(r"C:\Data")
        ));
        assert!(!is_within(
            Path::new(r"C:\Program Files2\App"),
            Path::new(r"C:\Program Files")
        ));
        assert!(same_path(Path::new(r"C:\A\B\"), Path::new(r"c:\a\b")));
    }

    #[test]
    fn plain_absolute_paths_reject_relative_unc_and_parent_components() {
        assert!(is_plain_absolute(Path::new(r"C:\Users\x\Downloads")));
        assert!(!is_plain_absolute(Path::new(r"Downloads")));
        assert!(!is_plain_absolute(Path::new(r"\\server\share\x")));
        assert!(!is_plain_absolute(Path::new(r"C:\a\..\Windows")));
        assert!(!is_plain_absolute(Path::new(r"C:relative")));
    }

    #[test]
    fn known_folders_resolve_and_are_protected() {
        let downloads = known_folder(KnownFolder::Downloads).unwrap();
        assert!(protected_roots().iter().any(|p| same_path(p, &downloads)));
        assert!(is_drive_root(Path::new(r"C:\")));
        assert!(!is_drive_root(Path::new(r"C:\Data")));
    }

    #[test]
    fn verbatim_prefix_is_added_once() {
        assert_eq!(verbatim(Path::new(r"C:\a")), PathBuf::from(r"\\?\C:\a"));
        assert_eq!(verbatim(Path::new(r"\\?\C:\a")), PathBuf::from(r"\\?\C:\a"));
    }
}
