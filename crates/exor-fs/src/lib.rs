//! Safe interfaces to the small Win32 surface needed for handle-bound file moves.

#![cfg(windows)]

use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind};
use std::mem::{offset_of, size_of, size_of_val};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, Prefix};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_DISPOSITION_INFO, FILE_ID_INFO, FILE_RENAME_INFO,
    FILE_STREAM_INFO, FileDispositionInfo, FileIdInfo, FileRenameInfo, FileStreamInfo,
    GetDriveTypeW, GetFileInformationByHandle, GetFileInformationByHandleEx,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, SetFileInformationByHandle,
};
use windows::core::PCWSTR;

fn io_error(error: windows::core::Error) -> io::Error {
    let code = error.code().0 as u32;
    if code >> 16 == 0x8007 {
        io::Error::from_raw_os_error((code & 0xffff) as i32)
    } else {
        io::Error::other(error.to_string())
    }
}

fn handle(file: &File) -> HANDLE {
    HANDLE(file.as_raw_handle())
}

/// Local volume serial and a 128-bit identifier. Not derived from a path spelling or timestamps.
pub type Identity = (u64, [u8; 16]);

/// Reject network and unmapped drives, even if a path has a drive letter rather than a UNC prefix.
pub fn is_local_drive(path: &Path) -> bool {
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return false;
    };
    let drive = match prefix.kind() {
        Prefix::Disk(d) | Prefix::VerbatimDisk(d) => d,
        _ => return false,
    };
    let root = [u16::from(drive), 58, 92, 0];
    // SAFETY: a terminated three-character drive root lives for the synchronous query.
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
    matches!(drive_type, 2 | 3 | 5 | 6) // removable, fixed, CD-ROM, RAM disk
}

fn id_info(file: &File) -> io::Result<FILE_ID_INFO> {
    let mut info = FILE_ID_INFO::default();
    // SAFETY: `file` owns a live handle and `info` is a sized and aligned output buffer.
    unsafe {
        GetFileInformationByHandleEx(
            handle(file),
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    }
    .map_err(io_error)?;
    Ok(info)
}

/// Query the identity of an open file and reject multiply-linked files for reviewed cleanup.
pub fn identity(file: &File) -> io::Result<Identity> {
    let info = id_info(file)?;
    let mut basic = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: both the live handle and the correctly sized POD output are valid for the call.
    unsafe { GetFileInformationByHandle(handle(file), &mut basic) }.map_err(io_error)?;
    if basic.nNumberOfLinks != 1 {
        return Err(io::Error::other(
            "multiply-linked file; not eligible for quarantine",
        ));
    }
    Ok((info.VolumeSerialNumber, info.FileId.Identifier))
}

fn flags(file: &File, dir: bool) -> io::Result<()> {
    let meta = file.metadata()?;
    if meta.file_attributes() & 0x400 != 0 || (if dir { !meta.is_dir() } else { !meta.is_file() }) {
        return Err(io::Error::other("redirected path or unexpected file type"));
    }
    Ok(())
}

/// Hold every directory component without delete sharing so no ancestor can be moved or
/// replaced during an operation. Paths must be validated local drive paths by the caller.
pub fn pin_parents(parent: &Path) -> io::Result<Vec<File>> {
    let mut paths: Vec<_> = parent.ancestors().filter(|p| p.has_root()).collect();
    let mut pinned = Vec::with_capacity(paths.len());
    while let Some(p) = paths.pop() {
        let dir = OpenOptions::new()
            .access_mode(0x80)
            .share_mode(0x1 | 0x2)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(p)?; // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
        flags(&dir, true)?;
        pinned.push(dir);
    }
    Ok(pinned)
}

/// Query the volume used by a directory with its parents pinned.
pub fn directory_volume(path: &Path) -> io::Result<u64> {
    let dirs = pin_parents(path)?;
    dirs.last()
        .ok_or_else(|| io::Error::from(ErrorKind::InvalidInput))
        .and_then(id_info)
        .map(|info| info.VolumeSerialNumber)
}

/// Open the source for reading and optionally for a by-handle rename/delete. No write/delete sharing.
pub fn open_locked(path: &Path, rename: bool) -> io::Result<File> {
    let access = 0x8000_0000 | if rename { 0x0001_0000 } else { 0 }; // GENERIC_READ | DELETE
    let file = OpenOptions::new()
        .access_mode(access)
        .share_mode(0x1)
        .custom_flags(0x0020_0000)
        .open(path)?;
    flags(&file, false)?;
    Ok(file)
}

fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let chars: Vec<_> = path.as_os_str().encode_wide().collect();
    if !path.is_absolute() || chars.len() > 32_760 || chars.contains(&0) {
        return Err(io::Error::from(ErrorKind::InvalidInput));
    }
    Ok(chars)
}

/// Atomically rename the opened file (never a fresh source lookup), with no replacement and
/// no cross-volume copy fallback. The caller must pin and validate both sets of parents first.
pub fn rename_open(file: &File, dest: &Path) -> io::Result<()> {
    let name = wide(dest)?;
    let offset = offset_of!(FILE_RENAME_INFO, FileName);
    let name_bytes = size_of_val(name.as_slice());
    // Win32's path conversion needs a NUL after the filename, but FileNameLength excludes it.
    let bytes = (offset + name_bytes + size_of::<u16>()).max(size_of::<FILE_RENAME_INFO>());
    // Zero-initialized storage includes the terminator and is aligned for the Windows structure.
    let mut buffer = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: a suitably aligned allocation holds the full header and trailing name. Windows
    // consumes the buffer synchronously; RootDirectory and ReplaceIfExists stay zero.
    unsafe {
        (*info).FileNameLength = name_bytes as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            info.cast::<u8>().add(offset).cast(),
            name.len(),
        );
        SetFileInformationByHandle(handle(file), FileRenameInfo, info.cast(), bytes as u32)
            .map_err(io_error)
    }
}

/// Delete the exact open file on handle close, not the result of another path lookup.
pub fn delete_open(file: &File) -> io::Result<()> {
    let request = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: the live handle has DELETE access and the Win32 input struct is initialized.
    unsafe {
        SetFileInformationByHandle(
            handle(file),
            FileDispositionInfo,
            (&request as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(io_error)
}

/// Refuse a stream-only copy when a file has named streams (or cannot be inspected).
/// Same-volume quarantine/restore rename preserves streams without requiring this restriction.
pub fn single_data_stream(file: &File) -> io::Result<()> {
    let mut storage = [0_u64; 512];
    // SAFETY: the handle is live and this aligned, initialized buffer is valid for the query.
    unsafe {
        GetFileInformationByHandleEx(
            handle(file),
            FileStreamInfo,
            storage.as_mut_ptr().cast(),
            size_of_val(&storage) as u32,
        )
    }
    .map_err(io_error)?;
    let info = storage.as_ptr().cast::<FILE_STREAM_INFO>();
    const DEFAULT_STREAM: [u16; 7] = [58, 58, 36, 68, 65, 84, 65]; // ::$DATA
    // SAFETY: validate the reported byte length before reading the trailing UTF-16 string.
    let is_default = unsafe {
        (*info).NextEntryOffset == 0
            && (*info).StreamNameLength as usize == size_of_val(&DEFAULT_STREAM)
            && std::slice::from_raw_parts(
                storage
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset_of!(FILE_STREAM_INFO, StreamName))
                    .cast::<u16>(),
                DEFAULT_STREAM.len(),
            ) == DEFAULT_STREAM
    };
    if !is_default {
        return Err(io::Error::other(
            "named or unsupported streams; a byte-only backup would be incomplete",
        ));
    }
    Ok(())
}

/// Publish a complete file/directory on the same volume; optionally replace our own journal.
pub fn publish(src: &Path, dest: &Path, replace: bool) -> io::Result<()> {
    let src: Vec<_> = wide(src)?.into_iter().chain([0]).collect();
    let dst: Vec<_> = wide(dest)?.into_iter().chain([0]).collect();
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            Default::default()
        };
    // SAFETY: the two valid NUL-terminated strings remain alive for the synchronous call.
    unsafe { MoveFileExW(PCWSTR(src.as_ptr()), PCWSTR(dst.as_ptr()), flags).map_err(io_error) }
}
