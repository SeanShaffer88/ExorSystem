//! Reads the Windows uninstall list (the data behind "Apps & features").

use crate::WinError;
use exor_engine::programs::{InstalledProgram, Scope, format_install_date};
use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    REG_DWORD, REG_EXPAND_SZ, REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegEnumKeyExW,
    RegOpenKeyExW, RegQueryValueExW,
};
use windows::core::{PCWSTR, PWSTR};

const UNINSTALL: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Closes a registry key on drop.
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by RegOpenKeyExW and is closed exactly once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> Option<Key> {
    let p = wide(path);
    let mut out = HKEY::default();
    // SAFETY: `p` is NUL-terminated and outlives the call; `out` is a valid out-pointer.
    let rc = unsafe { RegOpenKeyExW(root, PCWSTR(p.as_ptr()), None, KEY_READ | view, &mut out) };
    (rc == ERROR_SUCCESS).then_some(Key(out))
}

/// Raw value bytes and type, or `None` if missing.
fn raw(key: &Key, name: &str) -> Option<(Vec<u8>, REG_VALUE_TYPE)> {
    let n = wide(name);
    let mut ty = REG_VALUE_TYPE(0);
    let mut len = 0u32;
    // SAFETY: size query; all pointers reference live locals.
    let rc = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(n.as_ptr()),
            None,
            Some(&mut ty),
            None,
            Some(&mut len),
        )
    };
    if rc != ERROR_SUCCESS || len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` holds `len` writable bytes; `len` is passed as its size.
    let rc = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(n.as_ptr()),
            None,
            Some(&mut ty),
            Some(buf.as_mut_ptr()),
            Some(&mut len),
        )
    };
    (rc == ERROR_SUCCESS).then(|| {
        buf.truncate(len as usize);
        (buf, ty)
    })
}

fn string(key: &Key, name: &str) -> Option<String> {
    let (b, ty) = raw(key, name)?;
    if ty != REG_SZ && ty != REG_EXPAND_SZ {
        return None;
    }
    let units: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    let s = String::from_utf16_lossy(&units[..end]).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn dword(key: &Key, name: &str) -> Option<u32> {
    let (b, ty) = raw(key, name)?;
    (ty == REG_DWORD && b.len() >= 4).then(|| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn subkeys(key: &Key) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0.. {
        let mut buf = [0u16; 256];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` is a writable 256-unit buffer and `len` its capacity in units.
        let rc = unsafe {
            RegEnumKeyExW(
                key.0,
                i,
                Some(PWSTR(buf.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
        };
        if rc == ERROR_NO_MORE_ITEMS {
            break;
        }
        if rc == ERROR_SUCCESS {
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        // other errors (e.g. a key too long for the buffer): skip this index and keep going
        if i > 100_000 {
            break;
        }
    }
    out
}

fn read_root(
    root: HKEY,
    view: REG_SAM_FLAGS,
    scope: Scope,
    label: &str,
    out: &mut Vec<InstalledProgram>,
) {
    let Some(parent) = open(root, UNINSTALL, view) else {
        return;
    };
    for name in subkeys(&parent) {
        let Some(k) = open(root, &format!("{UNINSTALL}\\{name}"), view) else {
            continue;
        };
        let Some(display) = string(&k, "DisplayName") else {
            continue;
        };
        out.push(InstalledProgram {
            id: format!("{label}\\{name}"),
            name: display,
            version: string(&k, "DisplayVersion"),
            publisher: string(&k, "Publisher"),
            install_location: string(&k, "InstallLocation"),
            uninstall_command: string(&k, "UninstallString"),
            quiet_uninstall_command: string(&k, "QuietUninstallString"),
            install_date: string(&k, "InstallDate").and_then(|d| format_install_date(&d)),
            size_kb: dword(&k, "EstimatedSize").map(u64::from),
            scope,
            system_component: dword(&k, "SystemComponent") == Some(1),
            parent_key: string(&k, "ParentKeyName"),
            windows_installer: dword(&k, "WindowsInstaller") == Some(1),
        });
    }
}

/// Every uninstall entry from HKLM (64- and 32-bit views) and HKCU. Blocking.
pub fn enumerate_programs() -> Result<Vec<InstalledProgram>, WinError> {
    let mut out = Vec::new();
    read_root(
        HKEY_LOCAL_MACHINE,
        KEY_WOW64_64KEY,
        Scope::Machine,
        "HKLM",
        &mut out,
    );
    read_root(
        HKEY_LOCAL_MACHINE,
        KEY_WOW64_32KEY,
        Scope::Machine32,
        "HKLM32",
        &mut out,
    );
    read_root(
        HKEY_CURRENT_USER,
        REG_SAM_FLAGS(0),
        Scope::User,
        "HKCU",
        &mut out,
    );
    Ok(out)
}
