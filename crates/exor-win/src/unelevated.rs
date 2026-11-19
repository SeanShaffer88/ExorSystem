//! Start a program without administrator rights from the elevated app, as the signed-in desktop
//! user, and wait for it to finish.
//!
//! Per-user uninstall entries (HKCU) can be written by any program the user runs without
//! elevation, so ExorSystem must never run them with its own administrator token. This borrows the
//! token of the desktop shell (Explorer), the documented "run as desktop user" technique:
//! `GetShellWindow` -> shell process -> duplicate its token -> `CreateProcessWithTokenW`.

use crate::WinError;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Security::{
    DuplicateTokenEx, EqualSid, GetTokenInformation, SecurityImpersonation, TOKEN_ADJUST_DEFAULT,
    TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY, TOKEN_USER,
    TokenPrimary, TokenUser,
};
use windows::Win32::System::Threading::{
    CREATE_PROCESS_LOGON_FLAGS, CreateProcessWithTokenW, GetCurrentProcess, GetExitCodeProcess,
    INFINITE, OpenProcess, OpenProcessToken, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};
use windows::core::{PCWSTR, PWSTR};

fn werr(call: &'static str, e: &windows::core::Error) -> WinError {
    WinError::Win32 {
        call,
        message: e.message(),
        code: e.code().0 as u32,
    }
}

/// Closes a kernel handle on drop.
struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle was returned by a Win32 call and is closed exactly once.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

/// The token's user SID, as raw bytes in an 8-byte aligned buffer.
fn token_user(token: &Owned) -> Result<Vec<u64>, WinError> {
    let mut need = 0u32;
    // SAFETY: size query with no buffer; `need` is a live out-pointer.
    let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut need) };
    if need == 0 {
        return Err(WinError::Refused(
            "could not read which account the desktop belongs to".into(),
        ));
    }
    let mut buf = vec![0u64; (need as usize).div_ceil(8)];
    // SAFETY: `buf` is at least `need` bytes, aligned for TOKEN_USER; `need` is its size.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            need,
            &mut need,
        )
    }
    .map_err(|e| werr("GetTokenInformation", &e))?;
    Ok(buf)
}

/// Same account behind both tokens.
fn same_user(a: &Owned, b: &Owned) -> Result<bool, WinError> {
    let (ua, ub) = (token_user(a)?, token_user(b)?);
    // SAFETY: both buffers hold an initialised TOKEN_USER written by GetTokenInformation, whose
    // SIDs point into the same buffers, which outlive the comparison.
    let equal = unsafe {
        let (ta, tb) = (
            &*ua.as_ptr().cast::<TOKEN_USER>(),
            &*ub.as_ptr().cast::<TOKEN_USER>(),
        );
        EqualSid(ta.User.Sid, tb.User.Sid).is_ok()
    };
    Ok(equal)
}

/// Run `command_line` (program first, already quoted) as the desktop user, without administrator
/// rights, and wait for it to exit. Returns the exit code. Blocking: call from `spawn_blocking`.
///
/// Refuses, rather than falling back to running elevated, when there is no desktop shell or it
/// belongs to a different account than ExorSystem (an administrator who elevated it for a
/// standard user).
pub fn run_unelevated(command_line: &str) -> Result<u32, WinError> {
    // SAFETY: no arguments; returns null when no shell is running.
    let shell = unsafe { GetShellWindow() };
    if shell.0.is_null() {
        return Err(WinError::Refused(
            "the Windows desktop (Explorer) is not running, so the uninstaller cannot be started without administrator rights".into(),
        ));
    }
    let mut pid = 0u32;
    // SAFETY: `shell` is a window handle; `pid` is a live out-pointer.
    unsafe { GetWindowThreadProcessId(shell, Some(&mut pid)) };
    if pid == 0 {
        return Err(WinError::Refused(
            "could not find the desktop process".into(),
        ));
    }
    // SAFETY: plain call; the returned handle is owned and closed by `Owned`.
    let process = Owned(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map_err(|e| werr("OpenProcess", &e))?,
    );
    let mut shell_token = HANDLE::default();
    // SAFETY: `process` is a valid process handle; `shell_token` is a live out-pointer.
    unsafe { OpenProcessToken(process.0, TOKEN_DUPLICATE | TOKEN_QUERY, &mut shell_token) }
        .map_err(|e| werr("OpenProcessToken", &e))?;
    let shell_token = Owned(shell_token);

    let mut own_token = HANDLE::default();
    // SAFETY: the current-process pseudo handle is always valid; `own_token` is a live out-pointer.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut own_token) }
        .map_err(|e| werr("OpenProcessToken", &e))?;
    let own_token = Owned(own_token);
    if !same_user(&shell_token, &own_token)? {
        return Err(WinError::Refused(
            "this program was installed for the administrator account that started ExorSystem, not for the person signed in; sign in as that account to uninstall it".into(),
        ));
    }

    let mut primary = HANDLE::default();
    // SAFETY: `shell_token` was opened with TOKEN_DUPLICATE; `primary` is a live out-pointer.
    unsafe {
        DuplicateTokenEx(
            shell_token.0,
            TOKEN_QUERY
                | TOKEN_ASSIGN_PRIMARY
                | TOKEN_DUPLICATE
                | TOKEN_ADJUST_DEFAULT
                | TOKEN_ADJUST_SESSIONID,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut primary,
        )
    }
    .map_err(|e| werr("DuplicateTokenEx", &e))?;
    let primary = Owned(primary);

    // CreateProcessWithTokenW may write to the command line buffer, so it must be mutable.
    let mut cmd: Vec<u16> = command_line
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let si = STARTUPINFOW {
        cb: u32::try_from(std::mem::size_of::<STARTUPINFOW>()).unwrap_or(0),
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY: `primary` is a primary token with the rights the call needs; `cmd` is a writable,
    // NUL-terminated buffer that outlives the call; `si` and `pi` are live and correctly sized.
    unsafe {
        CreateProcessWithTokenW(
            primary.0,
            CREATE_PROCESS_LOGON_FLAGS(0),
            PCWSTR::null(),
            Some(PWSTR(cmd.as_mut_ptr())),
            PROCESS_CREATION_FLAGS(0),
            None,
            PCWSTR::null(),
            &si,
            &mut pi,
        )
    }
    .map_err(|e| werr("CreateProcessWithTokenW", &e))?;
    let (child, _thread) = (Owned(pi.hProcess), Owned(pi.hThread));

    // SAFETY: `child` is a valid process handle owned above.
    if unsafe { WaitForSingleObject(child.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err(WinError::Refused(
            "lost track of the uninstaller while waiting for it".into(),
        ));
    }
    let mut code = 0u32;
    // SAFETY: `child` is a valid process handle; `code` is a live out-pointer.
    unsafe { GetExitCodeProcess(child.0, &mut code) }
        .map_err(|e| werr("GetExitCodeProcess", &e))?;
    Ok(code)
}
