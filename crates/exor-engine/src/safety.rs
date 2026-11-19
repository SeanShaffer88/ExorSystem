//! Pre-action safety checks with portable verdicts: whether a vendor uninstaller's Authenticode
//! signature allows running it, and which programs hold files open before a cleanup moves them.
//!
//! The Win32 calls live in `exor-win`; the decisions and wording here are plain, testable Rust.

use serde::{Deserialize, Serialize};

/// What Windows' signature check (WinVerifyTrust) said about an executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureStatus {
    /// Valid signature from a trusted publisher (embedded or through a Windows catalog).
    Signed,
    /// No signature at all.
    Unsigned,
    /// Signed, but the signature could not be trusted (expired without a timestamp, unknown root,
    /// chain problem) or could not be checked. Runs only after explicit confirmation.
    Untrusted,
    /// Must never run: the file was changed after signing, or its certificate was revoked,
    /// explicitly distrusted, or blocked by policy.
    Rejected,
}

/// Signature verdict shown in the uninstall preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureCheck {
    /// Verdict.
    pub status: SignatureStatus,
    /// Signer's display name when the signature is readable.
    pub publisher: Option<String>,
    /// Verified through a Windows catalog rather than an embedded signature.
    pub catalog: bool,
    /// Plain-language explanation.
    pub detail: String,
}

impl SignatureCheck {
    /// The check itself failed (file unreadable, API error); treated as untrusted.
    pub fn unknown(detail: impl Into<String>) -> Self {
        Self {
            status: SignatureStatus::Untrusted,
            publisher: None,
            catalog: false,
            detail: detail.into(),
        }
    }

    /// Whether the uninstaller may start. `confirmed` is the person's explicit choice to run an
    /// unsigned or untrusted uninstaller; a rejected one never runs.
    pub fn may_run(&self, confirmed: bool) -> Result<(), String> {
        match self.status {
            SignatureStatus::Signed => Ok(()),
            SignatureStatus::Unsigned | SignatureStatus::Untrusted if confirmed => Ok(()),
            SignatureStatus::Unsigned | SignatureStatus::Untrusted => Err(format!(
                "{} Confirm that you trust this uninstaller before running it.",
                self.detail
            )),
            SignatureStatus::Rejected => Err(format!(
                "{} ExorSystem will not run it. Use Windows Settings → Apps, or reinstall the program first.",
                self.detail
            )),
        }
    }
}

// WinVerifyTrust results (HRESULT values from WinError.h).
const TRUST_E_NOSIGNATURE: u32 = 0x800B_0100;
const TRUST_E_SUBJECT_FORM_UNKNOWN: u32 = 0x800B_0003;
const TRUST_E_PROVIDER_UNKNOWN: u32 = 0x800B_0001;
const TRUST_E_BAD_DIGEST: u32 = 0x8009_6010;
const TRUST_E_EXPLICIT_DISTRUST: u32 = 0x800B_0111;
const CERT_E_REVOKED: u32 = 0x800B_010C;
const CRYPT_E_SECURITY_SETTINGS: u32 = 0x8009_2026;
const CERT_E_EXPIRED: u32 = 0x800B_0101;
const CERT_E_UNTRUSTEDROOT: u32 = 0x800B_0109;
const CERT_E_CHAINING: u32 = 0x800B_010A;
const TRUST_E_SUBJECT_NOT_TRUSTED: u32 = 0x800B_0004;

/// Map a WinVerifyTrust result to a verdict and a plain-language reason.
pub fn classify(result: i32) -> (SignatureStatus, &'static str) {
    match u32::from_ne_bytes(result.to_ne_bytes()) {
        0 => (SignatureStatus::Signed, "The uninstaller is signed."),
        TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => (
            SignatureStatus::Unsigned,
            "The uninstaller has no digital signature.",
        ),
        TRUST_E_BAD_DIGEST => (
            SignatureStatus::Rejected,
            "The uninstaller was changed after it was signed.",
        ),
        CERT_E_REVOKED => (
            SignatureStatus::Rejected,
            "The uninstaller's signing certificate was revoked.",
        ),
        TRUST_E_EXPLICIT_DISTRUST => (
            SignatureStatus::Rejected,
            "Windows is set to distrust the uninstaller's signer.",
        ),
        CRYPT_E_SECURITY_SETTINGS => (
            SignatureStatus::Rejected,
            "This computer's security policy blocks the uninstaller.",
        ),
        CERT_E_EXPIRED => (
            SignatureStatus::Untrusted,
            "The uninstaller's signature expired and was not timestamped.",
        ),
        CERT_E_UNTRUSTEDROOT | CERT_E_CHAINING => (
            SignatureStatus::Untrusted,
            "The uninstaller is signed by a certificate Windows does not trust.",
        ),
        TRUST_E_SUBJECT_NOT_TRUSTED => (
            SignatureStatus::Untrusted,
            "Windows does not trust the uninstaller's signature.",
        ),
        _ => (
            SignatureStatus::Untrusted,
            "Windows could not verify the uninstaller's signature.",
        ),
    }
}

/// What kind of program holds a file, as Windows' Restart Manager reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockerKind {
    /// A program with a main window.
    Window,
    /// A program without a main window.
    Console,
    /// A Windows service.
    Service,
    /// Windows Explorer.
    Explorer,
    /// A critical system process.
    Critical,
    /// Anything else.
    Other,
}

/// A program that has one of the files open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockingApp {
    /// Process id.
    pub pid: u32,
    /// Name Windows shows for it.
    pub name: String,
    /// Service short name, for services.
    pub service: Option<String>,
    /// Kind.
    pub kind: LockerKind,
}

/// Plain-language refusal listing the programs to close, or `None` when nothing holds the files.
/// ExorSystem never closes them itself.
pub fn lock_refusal(apps: &[LockingApp]) -> Option<String> {
    if apps.is_empty() {
        return None;
    }
    let mut names: Vec<String> = apps
        .iter()
        .map(|a| match (&a.service, a.kind) {
            (Some(s), _) => format!("{} (service {s})", a.name),
            (None, LockerKind::Explorer) => format!("{} (File Explorer)", a.name),
            _ => a.name.clone(),
        })
        .collect();
    names.sort();
    names.dedup();
    let shown = names.len().min(8);
    let more = names.len() - shown;
    let list = names[..shown].join(", ");
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    Some(format!(
        "Some of these files are open in {list}{tail}. Close {} and choose Apply again; nothing was moved.",
        if names.len() == 1 { "it" } else { "them" }
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn hr(v: u32) -> i32 {
        i32::from_ne_bytes(v.to_ne_bytes())
    }

    #[test]
    fn classifies_trust_results() {
        assert_eq!(classify(0).0, SignatureStatus::Signed);
        assert_eq!(
            classify(hr(TRUST_E_NOSIGNATURE)).0,
            SignatureStatus::Unsigned
        );
        assert_eq!(
            classify(hr(TRUST_E_BAD_DIGEST)).0,
            SignatureStatus::Rejected
        );
        assert_eq!(classify(hr(CERT_E_REVOKED)).0, SignatureStatus::Rejected);
        assert_eq!(
            classify(hr(TRUST_E_EXPLICIT_DISTRUST)).0,
            SignatureStatus::Rejected
        );
        assert_eq!(classify(hr(CERT_E_EXPIRED)).0, SignatureStatus::Untrusted);
        assert_eq!(classify(hr(0x8000_4005)).0, SignatureStatus::Untrusted);
    }

    #[test]
    fn only_signed_runs_without_confirmation_and_rejected_never_runs() {
        let check = |status| SignatureCheck {
            status,
            publisher: None,
            catalog: false,
            detail: "x".into(),
        };
        assert!(check(SignatureStatus::Signed).may_run(false).is_ok());
        for s in [SignatureStatus::Unsigned, SignatureStatus::Untrusted] {
            assert!(check(s).may_run(false).is_err());
            assert!(check(s).may_run(true).is_ok());
        }
        assert!(check(SignatureStatus::Rejected).may_run(true).is_err());
        assert!(
            SignatureCheck::unknown("unreadable")
                .may_run(false)
                .is_err()
        );
    }

    #[test]
    fn lock_refusal_lists_programs_to_close() {
        assert_eq!(lock_refusal(&[]), None);
        let app = |name: &str, service: Option<&str>, kind| LockingApp {
            pid: 1,
            name: name.into(),
            service: service.map(Into::into),
            kind,
        };
        let msg = lock_refusal(&[
            app("Foo Editor", None, LockerKind::Window),
            app("Foo Editor", None, LockerKind::Window),
            app("Foo Updater", Some("FooSvc"), LockerKind::Service),
        ])
        .unwrap();
        assert!(
            msg.contains("Foo Editor, Foo Updater (service FooSvc)"),
            "{msg}"
        );
        assert!(msg.contains("Close them"), "{msg}");
        assert!(msg.contains("nothing was moved"), "{msg}");
        let many: Vec<_> = (0..10)
            .map(|i| app(&format!("App {i}"), None, LockerKind::Console))
            .collect();
        assert!(lock_refusal(&many).unwrap().contains("and 2 more"));
    }
}
