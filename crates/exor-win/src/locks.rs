//! Which programs have files open, through Windows' Restart Manager. Read-only: ExorSystem
//! reports the programs and never closes or restarts them.

use crate::WinError;
use exor_engine::safety::{LockerKind, LockingApp};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmConsole, RmCritical, RmEndSession, RmExplorer,
    RmGetList, RmMainWindow, RmOtherWindow, RmRegisterResources, RmService, RmStartSession,
};
use windows::core::{PCWSTR, PWSTR};

fn rm_err(call: &'static str, rc: WIN32_ERROR) -> WinError {
    let e = windows::core::Error::from(rc.to_hresult());
    WinError::Win32 {
        call,
        message: e.message(),
        code: e.code().0 as u32,
    }
}

/// Ends a Restart Manager session on drop.
struct Session(u32);
impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the handle came from RmStartSession and is ended exactly once.
        let _ = unsafe { RmEndSession(self.0) };
    }
}

fn text(units: &[u16]) -> String {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

fn app(p: &RM_PROCESS_INFO) -> LockingApp {
    let kind = match p.ApplicationType {
        t if t == RmMainWindow || t == RmOtherWindow => LockerKind::Window,
        t if t == RmConsole => LockerKind::Console,
        t if t == RmService => LockerKind::Service,
        t if t == RmExplorer => LockerKind::Explorer,
        t if t == RmCritical => LockerKind::Critical,
        _ => LockerKind::Other,
    };
    let service = text(&p.strServiceShortName);
    let mut name = text(&p.strAppName);
    if name.trim().is_empty() {
        name = format!("process {}", p.Process.dwProcessId);
    }
    LockingApp {
        pid: p.Process.dwProcessId,
        name,
        service: (!service.is_empty()).then_some(service),
        kind,
    }
}

/// Programs (other than ExorSystem) that have any of `paths` open. Blocking.
pub fn locking_apps(paths: &[PathBuf]) -> Result<Vec<LockingApp>, WinError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut handle = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: `handle` is a live out-pointer; `key` is CCH_RM_SESSION_KEY + 1 writable units.
    let rc = unsafe { RmStartSession(&mut handle, None, PWSTR(key.as_mut_ptr())) };
    if rc != ERROR_SUCCESS {
        return Err(rm_err("RmStartSession", rc));
    }
    let session = Session(handle);
    let wide: Vec<Vec<u16>> = paths
        .iter()
        .map(|p| {
            p.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect();
    let names: Vec<PCWSTR> = wide.iter().map(|w| PCWSTR(w.as_ptr())).collect();
    for chunk in names.chunks(256) {
        // SAFETY: every pointer refers to a NUL-terminated path in `wide`, alive for the call.
        let rc = unsafe { RmRegisterResources(session.0, Some(chunk), None, None) };
        if rc != ERROR_SUCCESS {
            return Err(rm_err("RmRegisterResources", rc));
        }
    }
    let own = std::process::id();
    let mut capacity = 0u32;
    // The list can grow between calls; retry a few times before giving up.
    for _ in 0..4 {
        let mut buf = vec![RM_PROCESS_INFO::default(); capacity as usize];
        let (mut needed, mut count, mut reasons) = (0u32, capacity, 0u32);
        let ptr = (capacity > 0).then_some(buf.as_mut_ptr());
        // SAFETY: `buf` holds `count` writable entries (none when `ptr` is None); out-pointers are live.
        let rc = unsafe { RmGetList(session.0, &mut needed, &mut count, ptr, &mut reasons) };
        if rc == ERROR_SUCCESS {
            buf.truncate(count as usize);
            return Ok(buf
                .iter()
                .filter(|p| p.Process.dwProcessId != own)
                .map(app)
                .collect());
        }
        if rc != ERROR_MORE_DATA {
            return Err(rm_err("RmGetList", rc));
        }
        capacity = needed.saturating_add(4);
    }
    Err(WinError::Coverage(
        "the list of programs using these files kept changing",
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn reports_a_program_holding_a_file_open_and_nothing_once_it_exits() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("held.txt");
        std::fs::write(&file, b"held").unwrap();
        assert!(
            locking_apps(std::slice::from_ref(&file))
                .unwrap()
                .is_empty()
        );
        // Hold the file open from another process: cmd keeps it as stdin while it waits.
        let mut child = Command::new("cmd.exe")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .stdin(Stdio::from(std::fs::File::open(&file).unwrap()))
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let found = locking_apps(std::slice::from_ref(&file)).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!found.is_empty(), "the holder was not reported");
        assert!(found.iter().all(|a| a.pid != std::process::id()));
        assert!(locking_apps(&[]).unwrap().is_empty());
    }
}
