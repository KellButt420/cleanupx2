//! File identity and mutations tied to the same owned Windows handle.
use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr::{null, null_mut};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Storage::FileSystem::*;

use crate::{CoreError, Result, paths::verbatim};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    pub size_bytes: u64,
    pub modified_ticks: u64,
    #[serde(default)]
    pub volume: Option<u32>,
    #[serde(default)]
    pub file_index: Option<u64>,
}

impl FileIdentity {
    pub fn of(path: &Path) -> Option<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .ok()?;
        Self::from_file(&file).ok()
    }

    pub fn from_file(file: &File) -> Result<Self> {
        let info = information(file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(CoreError::rejected(
                "Links cannot be used for this operation.",
            ));
        }
        Ok(Self {
            size_bytes: (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
            modified_ticks: (u64::from(info.ftLastWriteTime.dwHighDateTime) << 32)
                | u64::from(info.ftLastWriteTime.dwLowDateTime),
            volume: Some(info.dwVolumeSerialNumber),
            file_index: Some(
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            ),
        })
    }
}

fn information(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(CoreError::last_win32("Reading file identity"));
    }
    Ok(info)
}

/// Owns delete access while denying other opens for writing or deletion.
/// Renames and deletion use this handle, never another lookup of the source name.
pub struct LockedFile(File);

impl LockedFile {
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .access_mode(0x8000_0000 | 0x0001_0000)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(verbatim(path))
            .map_err(|e| io_error("Locking the file", e))?;
        FileIdentity::from_file(&file)?;
        Ok(Self(file))
    }

    pub fn identity(&self) -> Result<FileIdentity> {
        FileIdentity::from_file(&self.0)
    }
    pub fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        self.0.metadata()
    }

    pub fn move_to(
        self,
        destination: &Path,
        mut prepare: impl FnMut(FileIdentity) -> Result<()>,
    ) -> Result<FileIdentity> {
        let identity = self.identity()?;
        let parent = destination
            .parent()
            .ok_or_else(|| CoreError::invalid("Invalid destination."))?;
        let parent_id = FileIdentity::of(parent)
            .ok_or_else(|| CoreError::rejected("The destination cannot be inspected."))?;
        if identity.volume == parent_id.volume {
            prepare(identity)?;
            self.rename(destination)?;
            self.identity()
        } else {
            self.copy_to(destination, &mut prepare)
        }
    }

    fn rename(&self, destination: &Path) -> Result<()> {
        let name: Vec<u16> = crate::wide::os_to_wide(verbatim(destination).as_os_str());
        // An aligned allocation with the variable-length filename at its ABI offset.
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let bytes = offset + (name.len() - 1) * 2;
        let mut storage = vec![0u64; bytes.div_ceil(8)];
        let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            (*info).Anonymous.ReplaceIfExists = false;
            (*info).RootDirectory = null_mut();
            (*info).FileNameLength = ((name.len() - 1) * 2) as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                std::ptr::addr_of_mut!((*info).FileName).cast(),
                name.len() - 1,
            );
            if SetFileInformationByHandle(
                self.0.as_raw_handle(),
                FileRenameInfo,
                info.cast(),
                bytes as u32,
            ) == 0
            {
                return Err(CoreError::last_win32("Moving the verified file"));
            }
        }
        Ok(())
    }

    fn delete(&self) -> Result<()> {
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        if unsafe {
            SetFileInformationByHandle(
                self.0.as_raw_handle(),
                FileDispositionInfo,
                (&info as *const FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        } == 0
        {
            return Err(CoreError::last_win32("Removing the verified source"));
        }
        Ok(())
    }

    fn copy_to(
        self,
        destination: &Path,
        prepare: &mut impl FnMut(FileIdentity) -> Result<()>,
    ) -> Result<FileIdentity> {
        let source_info = information(&self.0)?;
        if source_info.dwFileAttributes & (FILE_ATTRIBUTE_ENCRYPTED | FILE_ATTRIBUTE_DIRECTORY) != 0
        {
            return Err(CoreError::rejected(
                "This file requires a Windows-specific copy method; it was left in place.",
            ));
        }
        let target = Self(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .access_mode(0xc001_0000)
                .share_mode(FILE_SHARE_READ)
                .open(verbatim(destination))
                .map_err(|e| io_error("Creating the exact destination", e))?,
        );
        let result = (|| {
            copy_streams(&self.0, &target.0)?;
            if unsafe {
                SetFileTime(
                    target.0.as_raw_handle(),
                    &source_info.ftCreationTime,
                    &source_info.ftLastAccessTime,
                    &source_info.ftLastWriteTime,
                )
            } == 0
            {
                return Err(CoreError::last_win32("Preserving file dates"));
            }
            target
                .0
                .sync_all()
                .map_err(|e| io_error("Saving the copy", e))?;
            let identity = target.identity()?;
            prepare(identity)?; // Durable identity BEFORE deleting the original.
            self.delete()?;
            Ok(identity)
        })();
        if result.is_err() {
            // Only the exact destination we created can be removed on failure.
            let _ = target.delete();
        }
        result
    }
}

struct BackupContext {
    handle: windows_sys::Win32::Foundation::HANDLE,
    context: *mut std::ffi::c_void,
    write: bool,
}
impl BackupContext {
    fn transfer(&mut self, bytes: &mut [u8]) -> Result<usize> {
        let mut count = 0;
        let ok = unsafe {
            if self.write {
                BackupWrite(
                    self.handle,
                    bytes.as_mut_ptr(),
                    bytes.len() as u32,
                    &mut count,
                    0,
                    0,
                    &mut self.context,
                )
            } else {
                BackupRead(
                    self.handle,
                    bytes.as_mut_ptr(),
                    bytes.len() as u32,
                    &mut count,
                    0,
                    0,
                    &mut self.context,
                )
            }
        };
        if ok == 0 {
            return Err(CoreError::last_win32("Copying file streams"));
        }
        Ok(count as usize)
    }
    fn exact(&mut self, bytes: &mut [u8]) -> Result<()> {
        let mut offset = 0;
        while offset < bytes.len() {
            let count = self.transfer(&mut bytes[offset..])?;
            if count == 0 {
                return Err(CoreError::rejected(
                    "A file stream was incomplete; the original was preserved.",
                ));
            }
            offset += count;
        }
        Ok(())
    }
}
impl Drop for BackupContext {
    fn drop(&mut self) {
        let mut count = 0;
        unsafe {
            if self.write {
                BackupWrite(self.handle, null(), 0, &mut count, 1, 0, &mut self.context);
            } else {
                BackupRead(
                    self.handle,
                    null_mut(),
                    0,
                    &mut count,
                    1,
                    0,
                    &mut self.context,
                );
            }
        }
    }
}

fn copy_streams(source: &File, destination: &File) -> Result<()> {
    let mut reader = BackupContext {
        handle: source.as_raw_handle(),
        context: null_mut(),
        write: false,
    };
    let mut writer = BackupContext {
        handle: destination.as_raw_handle(),
        context: null_mut(),
        write: true,
    };
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let mut header = [0u8; 20]; // WIN32_STREAM_ID fixed fields, without WCHAR tail/alignment.
        let count = reader.transfer(&mut header)?;
        if count == 0 {
            break;
        }
        reader.exact(&mut header[count..])?;
        let kind = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let size = i64::from_le_bytes(header[8..16].try_into().unwrap());
        let name_len = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
        if size < 0 || name_len > 64 * 1024 || name_len % 2 != 0 {
            return Err(CoreError::rejected(
                "Unsupported file stream; the original was preserved.",
            ));
        }
        // Never restore hard-link/reparse instructions or silently discard an unknown data stream.
        let copy = matches!(
            kind,
            BACKUP_DATA | BACKUP_EA_DATA | BACKUP_ALTERNATE_DATA | BACKUP_PROPERTY_DATA
        );
        if !copy && !matches!(kind, BACKUP_SECURITY_DATA | BACKUP_OBJECT_ID) {
            return Err(CoreError::rejected(
                "This file has a stream that cannot be moved safely across drives.",
            ));
        }
        let mut name = vec![0u8; name_len];
        reader.exact(&mut name)?;
        if copy {
            writer.exact(&mut header)?;
            writer.exact(&mut name)?;
        }
        let mut remaining = size as u64;
        while remaining > 0 {
            let length = remaining.min(chunk.len() as u64) as usize;
            reader.exact(&mut chunk[..length])?;
            if copy {
                writer.exact(&mut chunk[..length])?;
            }
            remaining -= length as u64;
        }
    }
    Ok(())
}

fn io_error(context: &str, error: std::io::Error) -> CoreError {
    CoreError::win32(context, error.raw_os_error().unwrap_or(5) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::path::absolute(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/handle-tests")
                .join(format!(
                    "cleanup-handle-test-{}-{}-{}",
                    std::process::id(),
                    crate::time::unix_millis_now(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                )),
        )
        .unwrap();
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn locked_source_cannot_be_replaced_but_can_be_renamed_by_its_owner() {
        let root = fixture();
        let source = root.join("a.txt");
        let target = root.join("b.txt");
        std::fs::write(&source, "approved").unwrap();
        let file = LockedFile::open(&source).unwrap();
        let approved = file.identity().unwrap();
        assert!(std::fs::rename(&source, root.join("parked.txt")).is_err());
        assert!(std::fs::write(&source, "replacement").is_err());
        let identity = file.move_to(&target, |_| Ok(())).unwrap();
        assert_eq!(identity, approved);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "approved");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn copy_preserves_named_streams_and_failed_prepare_preserves_original() {
        let root = fixture();
        let source = root.join("a.txt");
        let target = root.join("b.txt");
        std::fs::write(&source, "data").unwrap();
        std::fs::write(format!("{}:audit", source.display()), "named stream").unwrap();
        let mut called = false;
        let file = LockedFile::open(&source).unwrap();
        file.copy_to(&target, &mut |_| {
            called = true;
            Ok(())
        })
        .unwrap();
        assert!(called);
        assert!(!source.exists());
        assert_eq!(
            std::fs::read_to_string(format!("{}:audit", target.display())).unwrap(),
            "named stream"
        );
        assert!(
            LockedFile::open(&target)
                .unwrap()
                .copy_to(&source, &mut |_| Err(CoreError::rejected(
                    "injected journal failure"
                )))
                .is_err()
        );
        assert!(target.exists());
        assert!(!source.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
