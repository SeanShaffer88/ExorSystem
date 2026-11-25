//! Stored environment variables, read raw from the registry (`%NAME%` kept as written), and
//! Windows' own expansion. Read-only.

use exor_engine::envcheck::{EnvScope, EnvVar};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows::core::PCWSTR;

const USER_KEY: &str = "Environment";
const MACHINE_KEY: &str = "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment";
/// Larger than any value Windows accepts in an environment block (32,767 characters).
const MAX_VALUE_BYTES: u32 = 64 * 1024;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by RegOpenKeyExW and is closed exactly once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn open(root: HKEY, path: &str) -> Result<Option<Key>, String> {
    let p = wide(path);
    let mut out = HKEY::default();
    // SAFETY: `p` is NUL-terminated and outlives the call; `out` is a live out-pointer.
    let rc = unsafe { RegOpenKeyExW(root, PCWSTR(p.as_ptr()), None, KEY_READ, &mut out) };
    if rc == ERROR_SUCCESS {
        Ok(Some(Key(out)))
    } else if rc == ERROR_FILE_NOT_FOUND {
        Ok(None)
    } else {
        Err(format!(
            "could not open {path}: {}",
            std::io::Error::from_raw_os_error(rc.0 as i32)
        ))
    }
}

/// A string value and whether it is `REG_EXPAND_SZ`; `None` when missing or not a string.
fn read(key: &Key, name: &str) -> Option<(String, bool)> {
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
    if rc != ERROR_SUCCESS || (ty != REG_SZ && ty != REG_EXPAND_SZ) || len > MAX_VALUE_BYTES {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` holds `len` writable bytes and `len` is passed as its size.
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
    if rc != ERROR_SUCCESS {
        return None;
    }
    buf.truncate(len as usize);
    let units: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    Some((String::from_utf16_lossy(&units[..end]), ty == REG_EXPAND_SZ))
}

/// The named variables as stored for the current user and for the machine, user first.
/// Variables that aren't set are left out.
pub fn stored_variables(names: &[&str]) -> Result<Vec<EnvVar>, String> {
    let mut out = Vec::new();
    for (scope, root, path) in [
        (EnvScope::User, HKEY_CURRENT_USER, USER_KEY),
        (EnvScope::Machine, HKEY_LOCAL_MACHINE, MACHINE_KEY),
    ] {
        let Some(key) = open(root, path)? else {
            continue;
        };
        for name in names {
            if let Some((value, expandable)) = read(&key, name) {
                out.push(EnvVar {
                    scope,
                    name: (*name).to_string(),
                    value,
                    expandable,
                });
            }
        }
    }
    Ok(out)
}

/// Expand `%NAME%` references with this process's environment, the way Windows does.
/// Unknown names are left as written.
pub fn expand(value: &str) -> String {
    let src = wide(value);
    let mut buf = vec![0u16; 1024];
    for _ in 0..3 {
        // SAFETY: `src` is NUL-terminated; `buf` is a writable buffer whose length is passed.
        let n = unsafe { ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), Some(&mut buf)) } as usize;
        if n == 0 {
            return value.to_string();
        }
        if n <= buf.len() {
            return String::from_utf16_lossy(&buf[..n - 1]);
        }
        buf = vec![0u16; n];
    }
    value.to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_machine_path_raw_and_expands_like_windows() {
        let vars = stored_variables(&["Path"]).unwrap();
        let machine = vars
            .iter()
            .find(|v| v.scope == EnvScope::Machine)
            .expect("every Windows install has a machine Path");
        // Usually REG_EXPAND_SZ, but tools that set PATH through .NET store REG_SZ (as on GitHub's
        // runners), so compare with what reg.exe reports instead of assuming.
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let out = std::process::Command::new(
            std::path::Path::new(&root).join("System32").join("reg.exe"),
        )
        .args(["query", &format!("HKLM\\{MACHINE_KEY}"), "/v", "Path"])
        .output()
        .unwrap();
        let listing = String::from_utf8_lossy(&out.stdout);
        let reported = listing
            .lines()
            .find(|l| l.trim_start().to_lowercase().starts_with("path "))
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or_else(|| panic!("no Path in: {listing}"))
            .to_string();
        assert_eq!(
            machine.expandable,
            reported == "REG_EXPAND_SZ",
            "reg.exe says {reported}"
        );
        assert!(
            machine.value.to_lowercase().contains("system32"),
            "{}",
            machine.value
        );
        let root = std::env::var("SystemRoot").unwrap();
        assert_eq!(expand("%SystemRoot%\\x"), format!("{root}\\x"));
        assert_eq!(expand("%EXOR_NOT_SET_9%\\x"), "%EXOR_NOT_SET_9%\\x");
        assert_eq!(expand(""), "");
    }
}
