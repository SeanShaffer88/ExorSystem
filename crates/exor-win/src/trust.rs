//! Authenticode check of an executable before ExorSystem starts it: embedded signature first,
//! then the Windows catalogs (most of `System32`, including `msiexec.exe`, is catalog-signed).
//!
//! Offline by design: no revocation lookups and no network retrieval, only what Windows already
//! has cached. The verdicts and wording live in `exor_engine::safety`.

use exor_engine::safety::{SignatureCheck, SignatureStatus, classify};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows::Win32::Foundation::{HANDLE, HWND, INVALID_HANDLE_VALUE, TRUST_E_NOSIGNATURE};
use windows::Win32::Security::Cryptography::CERT_NAME_SIMPLE_DISPLAY_TYPE;
use windows::Win32::Security::Cryptography::Catalog::{
    CATALOG_INFO, CryptCATAdminAcquireContext2, CryptCATAdminCalcHashFromFileHandle2,
    CryptCATAdminEnumCatalogFromHash, CryptCATAdminReleaseCatalogContext,
    CryptCATAdminReleaseContext, CryptCATCatalogInfoFromContext,
};
use windows::Win32::Security::Cryptography::CertGetNameStringW;
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_CATALOG_INFO, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_DATA_UNION_CHOICE, WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL,
    WTD_CHOICE_CATALOG, WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_NONE, WTD_REVOKE_NONE,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvSignerFromChain,
    WTHelperProvDataFromStateData, WinVerifyTrustEx,
};
use windows::core::{PCWSTR, w};

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn size_of<T>() -> u32 {
    u32::try_from(std::mem::size_of::<T>()).unwrap_or(u32::MAX)
}

/// Signer display name from a verified state, if Windows exposes one.
fn signer_name(state: HANDLE) -> Option<String> {
    // SAFETY: `state` is the live WinVerifyTrust state handle; it is closed only after this returns.
    let prov = unsafe { WTHelperProvDataFromStateData(state) };
    if prov.is_null() {
        return None;
    }
    // SAFETY: `prov` is non-null provider data owned by the open state.
    let sgnr = unsafe { WTHelperGetProvSignerFromChain(prov, 0, false, 0) };
    if sgnr.is_null() {
        return None;
    }
    // SAFETY: `sgnr` is non-null and lives as long as the state.
    let sgnr = unsafe { &*sgnr };
    if sgnr.csCertChain == 0 || sgnr.pasCertChain.is_null() {
        return None;
    }
    // SAFETY: the chain holds at least one element, the leaf (signer) certificate.
    let cert = unsafe { (*sgnr.pasCertChain).pCert };
    if cert.is_null() {
        return None;
    }
    let mut buf = [0u16; 256];
    // SAFETY: `cert` is a live certificate context; `buf` is a writable buffer whose length is passed.
    let n =
        unsafe { CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf)) }
            as usize;
    (n > 1).then(|| String::from_utf16_lossy(&buf[..(n - 1).min(buf.len())]))
}

/// Run WinVerifyTrust for one subject and always close its state. Returns the result and signer.
fn verify_subject(
    choice: WINTRUST_DATA_UNION_CHOICE,
    subject: WINTRUST_DATA_0,
) -> (i32, Option<String>) {
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: choice,
        Anonymous: subject,
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_NONE,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let no_ui = HWND(INVALID_HANDLE_VALUE.0);
    // SAFETY: `data` is fully initialised and its subject pointer refers to a live struct owned by
    // the caller for the duration of both calls; `action` is a live GUID.
    let result = unsafe { WinVerifyTrustEx(no_ui, &mut action, &mut data) };
    let signer = if result == TRUST_E_NOSIGNATURE.0 {
        None
    } else {
        signer_name(data.hWVTStateData)
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: same structures; closing releases the state opened by the verify call.
    let _ = unsafe { WinVerifyTrustEx(no_ui, &mut action, &mut data) };
    (result, signer)
}

/// Releases a catalog admin context on drop.
struct CatAdmin(isize);
impl Drop for CatAdmin {
    fn drop(&mut self) {
        // SAFETY: the context came from CryptCATAdminAcquireContext2 and is released once.
        let _ = unsafe { CryptCATAdminReleaseContext(self.0, 0) };
    }
}

/// Verify the file through the system catalogs; `None` when no catalog lists it.
fn verify_catalog(path: &Path, member: &[u16]) -> Option<(i32, Option<String>)> {
    for algorithm in [w!("SHA256"), w!("SHA1")] {
        let file = std::fs::File::open(path).ok()?;
        let handle = HANDLE(file.as_raw_handle());
        let mut admin = 0isize;
        // SAFETY: `admin` is a live out-pointer; the algorithm is a static NUL-terminated string.
        if unsafe { CryptCATAdminAcquireContext2(&mut admin, None, algorithm, None, None) }.is_err()
        {
            continue;
        }
        let admin = CatAdmin(admin);
        let mut len = 0u32;
        // SAFETY: size query with no buffer; `handle` is an open file handle owned by `file`.
        let _ =
            unsafe { CryptCATAdminCalcHashFromFileHandle2(admin.0, handle, &mut len, None, None) };
        if len == 0 || len > 128 {
            continue;
        }
        let mut hash = vec![0u8; len as usize];
        // SAFETY: `hash` holds `len` writable bytes and `len` is passed as its size.
        if unsafe {
            CryptCATAdminCalcHashFromFileHandle2(
                admin.0,
                handle,
                &mut len,
                Some(hash.as_mut_ptr()),
                None,
            )
        }
        .is_err()
        {
            continue;
        }
        hash.truncate(len as usize);
        // SAFETY: `hash` is the computed file hash; no previous catalog context is passed.
        let cat = unsafe { CryptCATAdminEnumCatalogFromHash(admin.0, &hash, None, None) };
        if cat == 0 {
            continue;
        }
        let mut info = CATALOG_INFO {
            cbStruct: size_of::<CATALOG_INFO>(),
            ..Default::default()
        };
        // SAFETY: `cat` is a live catalog context and `info` is a correctly sized out-struct.
        let found = unsafe { CryptCATCatalogInfoFromContext(cat, &mut info, 0) }.is_ok();
        let result = found.then(|| {
            let tag: Vec<u16> = hash
                .iter()
                .flat_map(|b| format!("{b:02X}").encode_utf16().collect::<Vec<_>>())
                .chain(std::iter::once(0))
                .collect();
            let mut catalog = WINTRUST_CATALOG_INFO {
                cbStruct: size_of::<WINTRUST_CATALOG_INFO>(),
                pcwszCatalogFilePath: PCWSTR(info.wszCatalogFile.as_ptr()),
                pcwszMemberTag: PCWSTR(tag.as_ptr()),
                pcwszMemberFilePath: PCWSTR(member.as_ptr()),
                hMemberFile: handle,
                pbCalculatedFileHash: hash.as_mut_ptr(),
                cbCalculatedFileHash: len,
                hCatAdmin: admin.0,
                ..Default::default()
            };
            verify_subject(
                WTD_CHOICE_CATALOG,
                WINTRUST_DATA_0 {
                    pCatalog: &mut catalog,
                },
            )
        });
        // SAFETY: releases the catalog context returned by the enumeration above, exactly once.
        let _ = unsafe { CryptCATAdminReleaseCatalogContext(admin.0, cat, 0) };
        if result.is_some() {
            return result;
        }
    }
    None
}

fn check(
    status: SignatureStatus,
    publisher: Option<String>,
    catalog: bool,
    why: &str,
) -> SignatureCheck {
    let detail = match (&publisher, status) {
        (Some(p), SignatureStatus::Signed) => format!("The uninstaller is signed by {p}."),
        (Some(p), _) => format!("{why} (Signer: {p}.)"),
        (None, _) => why.to_string(),
    };
    SignatureCheck {
        status,
        publisher,
        catalog,
        detail,
    }
}

/// Check `path`'s Authenticode signature: embedded, else through a Windows catalog. Blocking.
pub fn verify_signature(path: &Path) -> SignatureCheck {
    let member = wide(path);
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return SignatureCheck::unknown(format!(
                "The uninstaller could not be opened to check its signature ({e})."
            ));
        }
    };
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>(),
        pcwszFilePath: PCWSTR(member.as_ptr()),
        hFile: HANDLE(file.as_raw_handle()),
        ..Default::default()
    };
    let (result, publisher) = verify_subject(WTD_CHOICE_FILE, WINTRUST_DATA_0 { pFile: &mut info });
    drop(file);
    if result != TRUST_E_NOSIGNATURE.0 {
        let (status, why) = classify(result);
        return check(status, publisher, false, why);
    }
    match verify_catalog(path, &member) {
        Some((result, publisher)) => {
            let (status, why) = classify(result);
            check(status, publisher, true, why)
        }
        None => {
            let (status, why) = classify(result);
            check(status, None, false, why)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn system32(name: &str) -> std::path::PathBuf {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        std::path::PathBuf::from(root).join("System32").join(name)
    }

    #[test]
    fn system_binaries_are_signed() {
        // msiexec.exe and cmd.exe ship signed by Microsoft (catalog or embedded).
        for exe in ["msiexec.exe", "cmd.exe"] {
            let c = verify_signature(&system32(exe));
            assert_eq!(c.status, SignatureStatus::Signed, "{exe}: {c:?}");
        }
    }

    #[test]
    fn an_unsigned_copy_is_unsigned_and_a_changed_signed_file_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        let plain = d.path().join("plain.exe");
        std::fs::write(&plain, b"MZ not a real program").unwrap();
        assert_eq!(verify_signature(&plain).status, SignatureStatus::Unsigned);

        // Take a Microsoft binary with an embedded (not catalog) signature and change one byte.
        let program_files =
            std::env::var_os("ProgramFiles").unwrap_or_else(|| "C:\\Program Files".into());
        let candidates = [
            std::path::PathBuf::from(&program_files).join("Windows Defender\\MpCmdRun.exe"),
            std::path::PathBuf::from(&program_files).join("Internet Explorer\\iexplore.exe"),
            system32("OneDriveSetup.exe"),
            system32("msiexec.exe"),
            system32("cmd.exe"),
        ];
        let Some(source) = candidates.iter().find(|p| {
            let c = verify_signature(p);
            c.status == SignatureStatus::Signed && !c.catalog
        }) else {
            eprintln!("no embedded-signature binary found to tamper with; skipped");
            return;
        };
        let mut bytes = std::fs::read(source).unwrap();
        let i = bytes.len() / 3;
        bytes[i] ^= 0xFF;
        let changed = d.path().join("changed.exe");
        std::fs::write(&changed, &bytes).unwrap();
        assert_eq!(verify_signature(&changed).status, SignatureStatus::Rejected);
    }

    #[test]
    fn a_missing_file_needs_confirmation() {
        let c = verify_signature(Path::new("C:\\does\\not\\exist.exe"));
        assert_eq!(c.status, SignatureStatus::Untrusted);
        assert!(c.may_run(false).is_err());
    }
}
