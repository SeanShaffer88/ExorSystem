//! Read-only registry metadata for an explicit install-watch scope. No driver is needed.

use exor_engine::registry::{RegistryHive, RegistryKeyState, RegistryScope};
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, FILETIME, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE,
    KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_LINK, REG_OPTION_OPEN_LINK, REG_SAM_FLAGS,
    REG_VALUE_TYPE, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryInfoKeyW, RegQueryValueExW,
};
use windows::core::{PCWSTR, PWSTR, w};

const MAX_KEYS: usize = 4_096;
const MAX_DEPTH: usize = 32;
const MAX_PATH_BYTES: usize = 1024 * 1024;

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: this handle was opened by RegOpenKeyExW and is closed exactly once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn error(call: &str, code: WIN32_ERROR) -> String {
    format!(
        "{call}: {}",
        std::io::Error::from_raw_os_error(code.0 as i32)
    )
}

fn open(parent: HKEY, name: &str, view: REG_SAM_FLAGS) -> Result<Key, WIN32_ERROR> {
    let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut handle = HKEY::default();
    // SAFETY: `name` is a live NUL-terminated string, the parent handle is valid, and `handle`
    // receives the opened handle. Only read access is requested.
    let rc = unsafe {
        RegOpenKeyExW(
            parent,
            PCWSTR(name.as_ptr()),
            Some(REG_OPTION_OPEN_LINK.0),
            KEY_QUERY_VALUE | KEY_ENUMERATE_SUB_KEYS | view,
            &mut handle,
        )
    };
    if rc == ERROR_SUCCESS {
        Ok(Key(handle))
    } else {
        Err(rc)
    }
}

// We open one component at a time without following links. A link anywhere in a scope invalidates
// that sample, rather than letting an unintended subtree into the watch or implying a deletion.
fn refuse_link(key: &Key) -> Result<(), String> {
    let mut ty = REG_VALUE_TYPE::default();
    let mut size = 0;
    // SAFETY: the key is open; type and size point to writable locals. Passing no data buffer
    // ensures that the link target (or any other value data) is not read.
    let rc = unsafe {
        RegQueryValueExW(
            key.0,
            w!("SymbolicLinkValue"),
            None,
            Some(&mut ty),
            None,
            Some(&mut size),
        )
    };
    match rc {
        ERROR_FILE_NOT_FOUND => Ok(()),
        ERROR_SUCCESS if ty == REG_LINK => {
            Err("the subtree contains a registry link; links are not followed".into())
        }
        ERROR_SUCCESS => Ok(()),
        code => Err(error("checking for a registry link", code)),
    }
}

fn metadata(key: &Key) -> Result<(u32, u64), String> {
    let mut children = 0;
    let mut time = FILETIME::default();
    // SAFETY: both out-pointers are live writable locals; unused outputs are omitted.
    let rc = unsafe {
        RegQueryInfoKeyW(
            key.0,
            None,
            None,
            None,
            Some(&mut children),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&mut time),
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(error("reading key metadata", rc));
    }
    let stamp = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Ok((children, stamp))
}

fn child_name(key: &Key, index: u32) -> Result<String, String> {
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` has `len` writable units, and the call's handle and size pointer are live.
    let rc = unsafe {
        RegEnumKeyExW(
            key.0,
            index,
            Some(PWSTR(buf.as_mut_ptr())),
            &mut len,
            None,
            None,
            None,
            None,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(error("enumerating subkeys", rc));
    }
    let name = buf
        .get(..len as usize)
        .and_then(|units| String::from_utf16(units).ok())
        .filter(|name| !name.is_empty() && !name.contains(['\\', '\0']));
    name.ok_or_else(|| "a key name could not be represented as a single component".into())
}

#[derive(Default)]
struct Snapshot {
    keys: Vec<RegistryKeyState>,
    path_bytes: usize,
}

impl Snapshot {
    fn visit(
        &mut self,
        key: &Key,
        view: REG_SAM_FLAGS,
        path: &mut String,
        depth: usize,
    ) -> Result<(), String> {
        if self.keys.len() >= MAX_KEYS || self.path_bytes + path.len() > MAX_PATH_BYTES {
            return Err("the subtree is too large; choose a smaller subtree".into());
        }
        let (children, last_write) = metadata(key)?;
        if (depth == MAX_DEPTH && children > 0) || children as usize >= MAX_KEYS - self.keys.len() {
            return Err("the subtree is too large or deep; choose a smaller subtree".into());
        }
        self.path_bytes += path.len();
        self.keys.push(RegistryKeyState {
            path: path.clone(),
            last_write,
        });
        let parent_len = path.len();
        for index in 0..children {
            let name = child_name(key, index)?;
            let child = open(key.0, &name, view).map_err(|rc| error("opening a subkey", rc))?;
            refuse_link(&child)?;
            path.push('\\');
            path.push_str(&name);
            self.visit(&child, view, path, depth + 1)?;
            path.truncate(parent_len);
        }
        // A concurrent change to this key during traversal is not a complete sample.
        if metadata(key)? != (children, last_write) {
            return Err(
                "the registry was changing during the snapshot; that subtree was skipped".into(),
            );
        }
        Ok(())
    }
}

/// Read key paths and last-write times for a validated subtree. Missing roots are empty; if any
/// part is inaccessible, linked, or over the scan limits, return an error instead of partial data.
pub fn capture_registry_keys(scope: &RegistryScope) -> Result<Vec<RegistryKeyState>, String> {
    let (root, view) = match scope.hive() {
        RegistryHive::CurrentUser => (HKEY_CURRENT_USER, REG_SAM_FLAGS(0)),
        RegistryHive::Machine => (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        RegistryHive::Machine32 => (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
    };
    let mut key: Option<Key> = None;
    for name in scope.subkey().split('\\') {
        let parent = key.as_ref().map_or(root, |key| key.0);
        let opened = match open(parent, name, view) {
            Ok(key) => key,
            Err(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => return Ok(Vec::new()),
            Err(code) => return Err(error("opening the registry subtree", code)),
        };
        refuse_link(&opened)?;
        key = Some(opened);
    }
    let key = key.ok_or_else(|| "the registry scope was empty".to_string())?;
    let mut snapshot = Snapshot::default();
    snapshot.visit(&key, view, &mut scope.to_string(), 0)?;
    Ok(snapshot.keys)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use exor_engine::registry::RegistryScopes;

    #[test]
    fn reads_a_real_key_and_treats_a_missing_scope_as_empty() {
        // Read metadata only from a standard Windows key; do not create test data in the registry.
        let path = r"HKLM\Software\Microsoft\Windows NT\CurrentVersion\Time Zones";
        let scopes = RegistryScopes::parse(path).unwrap();
        let keys = capture_registry_keys(&scopes.as_slice()[0]).unwrap();
        assert_eq!(keys[0].path, path);
        assert_ne!(keys[0].last_write, 0);
        let utc = format!(r"{path}\UTC");
        assert!(keys.iter().any(|key| key.path == utc));

        let not_present = format!(
            r"HKCU\Software\ExorSystem-Missing-Registry-Snapshot-{}",
            std::process::id()
        );
        let scopes = RegistryScopes::parse(&not_present).unwrap();
        assert!(
            capture_registry_keys(&scopes.as_slice()[0])
                .unwrap()
                .is_empty()
        );
    }
}
