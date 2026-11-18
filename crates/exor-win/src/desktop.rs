//! Windows desktop boundaries: a normal user token and native known folders.

use std::path::{Component, Path, PathBuf, Prefix};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_System, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::{GUID, HSTRING, w};

struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: this is the real, owned token handle from OpenProcessToken, not a pseudo-handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Detect elevation before the desktop opens storage or registers mutating commands.
/// An unreadable token is an error, not permission to proceed.
pub fn process_is_elevated() -> Result<bool, String> {
    let mut handle = HANDLE::default();
    // SAFETY: the current-process pseudo-handle is valid, and handle points to writable storage.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) }
        .map_err(|e| format!("could not check the Windows process token: {e}"))?;
    let token = Token(handle);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut len = 0;
    // SAFETY: token is live; the passed buffer is sized for TokenElevation and len is writable.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    }
    .map_err(|e| format!("could not check Windows elevation: {e}"))?;
    Ok(elevation.TokenIsElevated != 0)
}

fn known_folder(id: &GUID) -> Result<PathBuf, String> {
    // SAFETY: id is a live GUID; no token means resolve the folder for the current user.
    let text = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }
        .map_err(|e| format!("could not locate a Windows folder: {e}"))?;
    // SAFETY: the success result is a live NUL-terminated string allocated by the shell.
    let path = unsafe { text.to_string() };
    // SAFETY: the known-folder API transfers ownership of this COM-allocated buffer.
    unsafe {
        CoTaskMemFree(Some(text.0.cast()));
    }
    let path = PathBuf::from(path.map_err(|e| format!("Windows folder is not valid text: {e}"))?);
    if !drive_path(&path) {
        return Err("Windows returned a data or system folder that is not a drive path".into());
    }
    Ok(path)
}

fn drive_path(path: &Path) -> bool {
    path.is_absolute()
        && matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
        && !path.components().any(|c| match c {
            Component::ParentDir => true,
            Component::Normal(name) => name
                .to_str()
                .is_none_or(|name| name.contains(':') || name.ends_with(['.', ' '])),
            _ => false,
        })
}

/// Per-user storage, independent of installer format and writable without elevation.
pub fn user_data_dir() -> Result<PathBuf, String> {
    Ok(known_folder(&FOLDERID_LocalAppData)?.join("ExorSystem"))
}

/// Refuse an existing redirected profile or database. Never change permissions on old data.
pub fn prepare_user_data_dir() -> Result<PathBuf, String> {
    let dir = user_data_dir()?;
    let folders: Vec<_> = dir.ancestors().collect();
    for folder in folders.into_iter().rev() {
        if !plain_profile_path(folder, true)? {
            if folder != dir {
                return Err(format!(
                    "Windows profile folder is missing: {}",
                    folder.display()
                ));
            }
            std::fs::create_dir(folder)
                .map_err(|e| format!("could not create {}: {e}", folder.display()))?;
            if !plain_profile_path(folder, true)? {
                return Err(format!("Profile folder disappeared: {}", folder.display()));
            }
        }
    }
    for name in [
        "desktop.lock",
        "exor.db",
        "exor.db-shm",
        "exor.db-wal",
        "exor.db-journal",
    ] {
        plain_profile_path(&dir.join(name), false)?;
    }
    Ok(dir)
}

fn plain_profile_path(path: &Path, directory: bool) -> Result<bool, String> {
    use std::os::windows::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(meta)
            if meta.file_attributes() & 0x400 == 0
                && if directory {
                    meta.is_dir()
                } else {
                    meta.is_file()
                } =>
        {
            Ok(true)
        }
        Ok(_) => Err(format!(
            "ExorSystem will not open a redirected or unexpected profile path: {}",
            path.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("could not check {}: {e}", path.display())),
    }
}

/// Resolve a registered executable without searching the working directory or PATH.
/// A bare MSI executable name is resolved by Windows' system-folder API.
pub fn uninstall_executable(exe: &str) -> Result<PathBuf, String> {
    use std::os::windows::fs::MetadataExt;
    let exe = if exe.eq_ignore_ascii_case("msiexec.exe") {
        known_folder(&FOLDERID_System)?.join("msiexec.exe")
    } else {
        PathBuf::from(exe)
    };
    let unsupported = || {
        "This uninstaller cannot be started safely. Use Windows Settings → Apps instead."
            .to_string()
    };
    if !drive_path(&exe)
        || !exe
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Err(unsupported());
    }
    let name = exe
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or_else(unsupported)?;
    if [
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "wscript.exe",
        "cscript.exe",
        "mshta.exe",
        "rundll32.exe",
        "regsvr32.exe",
    ]
    .iter()
    .any(|host| name.eq_ignore_ascii_case(host))
    {
        return Err(unsupported());
    }
    for part in exe.ancestors().filter(|part| part.parent().is_some()) {
        let meta = std::fs::symlink_metadata(part).map_err(|_| unsupported())?;
        if meta.file_attributes() & 0x400 != 0 || (part == exe && !meta.is_file()) {
            return Err(unsupported());
        }
    }
    Ok(exe)
}

/// Surface startup errors even in the Windows subsystem build with no console.
pub fn show_startup_error(error: &str) {
    // SAFETY: both text buffers live for the call and the message box has no parent window.
    let _ = unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(error),
            w!("ExorSystem"),
            MB_ICONERROR | MB_OK,
        )
    };
}

/// Decode a registered command using Windows' own quoting rules, never a shell. An unquoted
/// path containing spaces is unsupported: the first token must itself be an allowed executable.
pub fn uninstall_arguments(command: &str) -> Result<(PathBuf, Vec<String>), String> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::UI::Shell::CommandLineToArgvW;
    let command = command.trim();
    let invalid =
        "The uninstaller command is ambiguous or unsupported. Use Windows Settings → Apps instead.";
    if command.is_empty()
        || command.encode_utf16().count() > 32_766
        || command.contains(['\0', '\r', '\n'])
    {
        return Err(invalid.into());
    }
    // Validate argv[0] independently; Windows' argument parser is intentionally permissive.
    if let Some(quoted) = command.strip_prefix('"') {
        let (_, tail) = quoted.split_once('"').ok_or(invalid)?;
        if !tail.is_empty() && !tail.starts_with([' ', '\t']) {
            return Err(invalid.into());
        }
    } else if command
        .split([' ', '\t'])
        .next()
        .is_some_and(|first| first.contains('"'))
    {
        return Err(invalid.into());
    }
    let mut argc = 0;
    // SAFETY: a terminated UTF-16 string and a writable count are valid for the shell call.
    let argv = unsafe { CommandLineToArgvW(&HSTRING::from(command), &mut argc) };
    if argv.is_null() {
        return Err(invalid.into());
    }
    let values = if argc > 0 && argc <= 512 {
        // SAFETY: success returned `argc` live pointers to terminated UTF-16 strings.
        unsafe {
            std::slice::from_raw_parts(argv, argc as usize)
                .iter()
                .map(|p| p.to_string())
                .collect::<Result<Vec<_>, _>>()
        }
        .ok()
    } else {
        None
    };
    // SAFETY: CommandLineToArgvW transfers ownership of the single LocalAlloc buffer.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(argv.cast())));
    }
    let mut values = values.ok_or(invalid)?.into_iter();
    let exe = uninstall_executable(&values.next().ok_or(invalid)?)?;
    Ok((exe, values.collect()))
}

/// Choose an existing local folder or file, using an owned Windows dialog on a worker STA.
pub fn pick_path(owner: isize, directory: bool) -> Result<Option<String>, String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST,
        FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
    };
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            // SAFETY: balances the successful initialization on this same thread.
            unsafe { CoUninitialize() };
        }
    }
    // SAFETY: requested on this blocking worker and paired with CoUninitialize on success.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|e| e.to_string())?;
    let _apartment = Apartment;
    // SAFETY: only the system file dialog is instantiated; every returned COM value is local.
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| e.to_string())?;
        let options = FOS_FORCEFILESYSTEM
            | FOS_NOCHANGEDIR
            | FOS_PATHMUSTEXIST
            | if directory {
                FOS_PICKFOLDERS
            } else {
                FOS_FILEMUSTEXIST
            };
        dialog.SetOptions(options).map_err(|e| e.to_string())?;
        match dialog.Show(Some(HWND(owner as *mut _))) {
            Err(err) if err.code().0 as u32 == 0x8007_04c7 => return Ok(None),
            result => result.map_err(|e| e.to_string())?,
        }
        let item = dialog.GetResult().map_err(|e| e.to_string())?;
        let ptr = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| e.to_string())?;
        let path = ptr.to_string();
        CoTaskMemFree(Some(ptr.0.cast()));
        path.map(Some).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_process_token_and_resolves_the_actual_msi_executable() {
        assert!(process_is_elevated().is_ok());
        assert!(user_data_dir().unwrap().ends_with("ExorSystem"));
        let msi = uninstall_executable("MsiExec.exe").unwrap();
        assert!(msi.is_absolute() && msi.is_file());
    }

    #[test]
    fn refuses_ambiguous_executable_paths_without_launching_anything() {
        for path in [
            "tool.exe",
            "C:tool.exe",
            r"C:\temp\..\tool.exe",
            r"C:\temp\tool.cmd",
            r"C:\temp\cmd.exe",
            r"\\server\share\tool.exe",
        ] {
            assert!(uninstall_executable(path).is_err(), "{path}");
        }
    }
}
