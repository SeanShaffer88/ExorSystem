//! Enumerates Windows services and kernel-driver services through the Service Control Manager.

use crate::WinError;
use exor_engine::collectors::ServiceInfo;
use windows::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW,
    OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, SC_ENUM_PROCESS_INFO, SC_HANDLE,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_DEMAND_START,
    SERVICE_DISABLED, SERVICE_DRIVER, SERVICE_QUERY_CONFIG, SERVICE_STATE_ALL,
    SERVICE_SYSTEM_START, SERVICE_WIN32,
};
use windows::core::{PCWSTR, PWSTR};

fn werr(call: &'static str, e: &windows::core::Error) -> WinError {
    WinError::Win32 {
        call,
        message: e.message(),
        code: e.code().0 as u32,
    }
}

/// Closes an SCM handle on drop.
struct Handle(SC_HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by OpenSCManagerW/OpenServiceW and is closed exactly once.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

fn wide(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: the SCM returns NUL-terminated UTF-16 strings that live inside buffers we still hold.
    unsafe { p.to_string().unwrap_or_default() }
}

fn start_type(t: windows::Win32::System::Services::SERVICE_START_TYPE) -> &'static str {
    match t {
        x if x == SERVICE_BOOT_START => "boot",
        x if x == SERVICE_SYSTEM_START => "system",
        x if x == SERVICE_AUTO_START => "auto",
        x if x == SERVICE_DEMAND_START => "manual",
        x if x == SERVICE_DISABLED => "disabled",
        _ => "unknown",
    }
}

/// Binary path and start type of one service; `None` if it cannot be opened (access denied etc.).
fn config(scm: &Handle, name: &[u16]) -> Option<(String, &'static str)> {
    // SAFETY: `name` is a NUL-terminated UTF-16 service name; the handle is valid for the call.
    let svc =
        Handle(unsafe { OpenServiceW(scm.0, PCWSTR(name.as_ptr()), SERVICE_QUERY_CONFIG) }.ok()?);
    let mut need = 0u32;
    // SAFETY: a null buffer with size 0 is the documented way to ask for the required size.
    let _ = unsafe { QueryServiceConfigW(svc.0, None, 0, &mut need) };
    if need == 0 {
        return None;
    }
    // u64 backing keeps the buffer 8-byte aligned for QUERY_SERVICE_CONFIGW.
    let mut buf = vec![0u64; (need as usize).div_ceil(8)];
    let cfg = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
    // SAFETY: `buf` is at least `need` bytes and aligned; `need` is passed as its size.
    unsafe { QueryServiceConfigW(svc.0, Some(cfg), need, &mut need) }.ok()?;
    // SAFETY: the call succeeded, so `cfg` points to an initialised struct whose strings live in `buf`.
    let c = unsafe { &*cfg };
    Some((wide(c.lpBinaryPathName), start_type(c.dwStartType)))
}

/// All Win32 services and kernel drivers, sorted by name. Blocking: call from `spawn_blocking`.
pub fn enumerate_services() -> Result<Vec<ServiceInfo>, WinError> {
    // SAFETY: null machine/database selects the local active database.
    let scm = Handle(
        unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE) }
            .map_err(|e| werr("OpenSCManagerW", &e))?,
    );
    let mut need = 0u32;
    let mut count = 0u32;
    let mut resume = 0u32;
    // SAFETY: size query with no buffer; all out-pointers reference live locals.
    let _ = unsafe {
        EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32 | SERVICE_DRIVER,
            SERVICE_STATE_ALL,
            None,
            &mut need,
            &mut count,
            Some(&mut resume),
            PCWSTR::null(),
        )
    };
    let mut buf = vec![0u64; (need as usize + 4096).div_ceil(8)];
    let bytes = buf.len() * 8;
    // SAFETY: reinterpreting the aligned u64 buffer as bytes of the same length.
    let slice = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), bytes) };
    resume = 0;
    // SAFETY: `slice` is a valid writable buffer of `bytes` bytes; out-pointers reference live locals.
    unsafe {
        EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32 | SERVICE_DRIVER,
            SERVICE_STATE_ALL,
            Some(slice),
            &mut need,
            &mut count,
            Some(&mut resume),
            PCWSTR::null(),
        )
    }
    .map_err(|e| werr("EnumServicesStatusExW", &e))?;

    let first = buf.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        // SAFETY: the call reported `count` consecutive entries starting at the buffer head.
        let e = unsafe { &*first.add(i) };
        let name = wide(e.lpServiceName);
        let mut w: Vec<u16> = name.encode_utf16().collect();
        w.push(0);
        let (binary_path, start) = config(&scm, &w).unwrap_or_default_cfg();
        out.push(ServiceInfo {
            name,
            display_name: wide(e.lpDisplayName),
            binary_path,
            start_type: start.to_string(),
        });
    }
    out.sort_by_key(|s| s.name.to_lowercase());
    Ok(out)
}

trait OrDefaultCfg {
    fn unwrap_or_default_cfg(self) -> (String, &'static str);
}
impl OrDefaultCfg for Option<(String, &'static str)> {
    fn unwrap_or_default_cfg(self) -> (String, &'static str) {
        self.unwrap_or((String::new(), "unknown"))
    }
}
