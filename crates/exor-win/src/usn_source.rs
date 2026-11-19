//! NTFS USN journal reader (`FSCTL_QUERY_USN_JOURNAL` / `FSCTL_READ_USN_JOURNAL`).
//!
//! Reading is a kernel-side wait: each `DeviceIoControl` blocks (on the tokio blocking pool,
//! never on a runtime thread) until data arrives or a 1 s timeout lets the loop notice the
//! consumer is gone. No sleep-polling. Requires administrator rights on the volume.

use crate::{WinError, usn::checked_usn_buffer};
use exor_engine::Cancel;
use exor_engine::events::{LiveEnd, LiveEventSource, RawEvent};
use std::collections::HashMap;
use std::ffi::c_void;
use tokio::sync::mpsc;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdType,
    GETFINALPATHNAMEBYHANDLE_FLAGS, GetFinalPathNameByHandleW, OPEN_EXISTING, OpenFileById,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V1, USN_JOURNAL_DATA_V0,
};
use windows::core::HSTRING;

const GENERIC_READ: u32 = 0x8000_0000;
const DIR_CACHE_MAX: usize = 50_000;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by a successful Win32 open call and is closed exactly once here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn werr(call: &'static str, e: &windows::core::Error) -> WinError {
    WinError::Win32 {
        call,
        message: e.message(),
        code: e.code().0 as u32,
    }
}

/// Reads the change journal of one NTFS volume.
#[derive(Debug, Clone)]
pub struct UsnJournalSource {
    volume: char,
}

impl UsnJournalSource {
    /// Source for a drive letter such as `'C'`.
    pub fn new(volume: char) -> Self {
        Self { volume }
    }

    /// Check that the journal can be opened (admin rights, NTFS, journal active).
    pub fn probe(volume: char) -> Result<(), WinError> {
        let h = open_volume(volume)?;
        query_journal(&h).map(|_| ())
    }
}

fn open_volume(letter: char) -> Result<Handle, WinError> {
    let path = HSTRING::from(format!("\\\\.\\{letter}:"));
    // SAFETY: `path` is a valid NUL-terminated wide string that outlives the call; other arguments are plain values.
    let h = unsafe {
        CreateFileW(
            &path,
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| werr("CreateFileW(volume)", &e))?;
    Ok(Handle(h))
}

fn query_journal(vol: &Handle) -> Result<USN_JOURNAL_DATA_V0, WinError> {
    let mut data = USN_JOURNAL_DATA_V0::default();
    let mut returned = 0u32;
    // SAFETY: the output buffer is a live, properly sized USN_JOURNAL_DATA_V0; no input buffer or OVERLAPPED is passed.
    unsafe {
        DeviceIoControl(
            vol.0,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some((&mut data as *mut USN_JOURNAL_DATA_V0).cast::<c_void>()),
            std::mem::size_of::<USN_JOURNAL_DATA_V0>() as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(|e| werr("FSCTL_QUERY_USN_JOURNAL", &e))?;
    Ok(data)
}

fn dir_path(vol: &Handle, frn: u64, cache: &mut HashMap<u64, String>) -> Option<String> {
    if let Some(p) = cache.get(&frn) {
        return Some(p.clone());
    }
    let desc = FILE_ID_DESCRIPTOR {
        dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
        Type: FileIdType,
        Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: frn as i64 },
    };
    // SAFETY: `desc` is a fully initialised descriptor living across the call; `vol.0` is a valid volume handle.
    let h = unsafe {
        OpenFileById(
            vol.0,
            &desc,
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            FILE_FLAG_BACKUP_SEMANTICS,
        )
    }
    .ok()?;
    let h = Handle(h);
    let mut buf = vec![0u16; 1024];
    // SAFETY: `buf` is a valid writable slice; the API writes at most buf.len() units.
    let n = unsafe { GetFinalPathNameByHandleW(h.0, &mut buf, GETFINALPATHNAMEBYHANDLE_FLAGS(0)) }
        as usize;
    if n == 0 || n > buf.len() {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..n]);
    let s = s.strip_prefix("\\\\?\\").unwrap_or(&s).to_string();
    if cache.len() >= DIR_CACHE_MAX {
        cache.clear();
    }
    cache.insert(frn, s.clone());
    Some(s)
}

fn run(volume: char, tx: &mpsc::Sender<RawEvent>, stop: &Cancel) -> Result<LiveEnd, WinError> {
    let vol = open_volume(volume)?;
    let journal = query_journal(&vol)?;
    let mut next = journal.NextUsn; // only changes from now on
    let mut cache: HashMap<u64, String> = HashMap::new();
    let mut buf = vec![0u8; 64 * 1024];
    while !tx.is_closed() && !stop.is_cancelled() {
        let req = READ_USN_JOURNAL_DATA_V1 {
            StartUsn: next,
            ReasonMask: u32::MAX,
            ReturnOnlyOnClose: 0,
            Timeout: 1,
            BytesToWaitFor: 1,
            UsnJournalID: journal.UsnJournalID,
            MinMajorVersion: 2,
            MaxMajorVersion: 2,
        };
        let mut returned = 0u32;
        // SAFETY: `req` and `buf` are live for the call; sizes passed match the buffers; no OVERLAPPED is used.
        unsafe {
            DeviceIoControl(
                vol.0,
                FSCTL_READ_USN_JOURNAL,
                Some((&req as *const READ_USN_JOURNAL_DATA_V1).cast::<c_void>()),
                std::mem::size_of::<READ_USN_JOURNAL_DATA_V1>() as u32,
                Some(buf.as_mut_ptr().cast::<c_void>()),
                buf.len() as u32,
                Some(&mut returned),
                None,
            )
        }
        .map_err(|e| werr("FSCTL_READ_USN_JOURNAL", &e))?;
        let Some((n, records)) = buf.get(..returned as usize).and_then(checked_usn_buffer) else {
            return Ok(LiveEnd::InvalidRecord);
        };
        if n < next {
            return Ok(LiveEnd::JournalGap);
        }
        next = n;
        for r in records {
            let Some(reason) = r.fs_reason() else {
                continue;
            };
            let Some(dir) = dir_path(&vol, r.parent_frn, &mut cache) else {
                return Ok(LiveEnd::InvalidRecord);
            };
            let ts_ms = crate::filetime_to_unix_ms(u64::try_from(r.filetime).unwrap_or(0));
            let ev = RawEvent::Fs {
                usn: u64::try_from(r.usn).unwrap_or(0),
                path: format!("{dir}\\{}", r.name),
                reason,
                process: None,
                ts_ms,
            };
            match tx.try_send(ev) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Closed(_)) => return Ok(LiveEnd::Stopped),
                Err(mpsc::error::TrySendError::Full(_)) => return Ok(LiveEnd::Backpressure),
            }
        }
    }
    Ok(LiveEnd::Stopped)
}

impl LiveEventSource for UsnJournalSource {
    fn start_live(
        self: Box<Self>,
        stop: Cancel,
    ) -> (mpsc::Receiver<RawEvent>, tokio::task::JoinHandle<LiveEnd>) {
        let (tx, rx) = mpsc::channel(4096);
        let done = tokio::task::spawn_blocking(move || match run(self.volume, &tx, &stop) {
            Ok(end) => end,
            Err(WinError::Win32 {
                code: 0x8007_049b | 0x8007_049a | 0x8007_049d,
                ..
            }) => LiveEnd::JournalGap,
            Err(e) => LiveEnd::Unavailable(e.to_string()),
        });
        (rx, done)
    }
}
