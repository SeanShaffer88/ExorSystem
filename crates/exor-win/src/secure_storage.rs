//! Windows paths used for files that must not be trusted from portable app data.

use crate::WinError;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_ProgramData, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::{BOOL, HSTRING, PCWSTR};

/// Resolve the system ProgramData folder through Windows, not an overridable environment value.
pub fn program_data_dir() -> Result<PathBuf, WinError> {
    // SAFETY: the known-folder GUID is static and the API owns the returned COM-allocated string.
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, KF_FLAG_DEFAULT, None) }
        .map_err(|e| WinError::Win32 {
            call: "SHGetKnownFolderPath",
            message: e.message(),
            code: e.code().0 as u32,
        })?;
    // SAFETY: a successful SHGetKnownFolderPath call returns a valid NUL-terminated UTF-16 path.
    let text = unsafe { path.to_string() };
    // SAFETY: `path` was allocated by SHGetKnownFolderPath and is freed exactly once.
    unsafe { CoTaskMemFree(Some(path.0.cast())) };
    let text = text.map_err(|_| WinError::Proto("ProgramData path is not valid UTF-16"))?;
    if text.is_empty() {
        return Err(WinError::Proto("ProgramData path is empty"));
    }
    Ok(PathBuf::from(text))
}

/// Restrict quarantine files to SYSTEM and Administrators, inheriting this ACL for new children.
///
/// The directory lives in ProgramData, but the explicit protected DACL also prevents other local
/// accounts from reading quarantined user files.
pub fn secure_quarantine_dir(path: &Path) -> Result<(), WinError> {
    let sddl = HSTRING::from("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: `sddl` is a valid NUL-terminated string and `descriptor` is a writable out-pointer.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &sddl,
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(|e| WinError::Win32 {
        call: "ConvertStringSecurityDescriptorToSecurityDescriptorW",
        message: e.message(),
        code: e.code().0 as u32,
    })?;

    let result = (|| {
        let mut present = BOOL(0);
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut defaulted = BOOL(0);
        // SAFETY: `descriptor` is the valid descriptor returned above; output pointers are live.
        unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
            .map_err(|e| WinError::Win32 {
            call: "GetSecurityDescriptorDacl",
            message: e.message(),
            code: e.code().0 as u32,
        })?;
        if !present.as_bool() || dacl.is_null() {
            return Err(WinError::Proto("quarantine DACL is empty"));
        }
        let path_wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        // SAFETY: `path` and the ACL descriptor remain live for the call; we set only the DACL.
        let code = unsafe {
            SetNamedSecurityInfoW(
                PCWSTR(path_wide.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(dacl.cast_const()),
                None,
            )
        };
        if code.0 != 0 {
            return Err(WinError::Win32 {
                call: "SetNamedSecurityInfoW",
                message: format!("Windows error 0x{:08X}", code.0),
                code: code.0,
            });
        }
        Ok(())
    })();

    // SAFETY: the descriptor was allocated by the SDDL conversion API and is freed once.
    unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    result
}
