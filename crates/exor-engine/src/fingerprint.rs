//! Installer family fingerprinting: family marker, silent switches, binary BLAKE3.

use crate::error::{Result, io_err};
use crate::hashing::hash_file_blocking;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

/// Known installer technologies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallerFamily {
    /// Windows Installer package.
    Msi,
    /// Nullsoft Scriptable Install System.
    Nsis,
    /// Inno Setup.
    InnoSetup,
    /// InstallShield.
    InstallShield,
    /// WiX Burn bootstrapper.
    WixBurn,
    /// 7-Zip self-extractor.
    SevenZipSfx,
    /// Squirrel.Windows.
    Squirrel,
    /// No marker recognised.
    Unknown,
}

impl InstallerFamily {
    /// Stable text form stored in `fingerprint_family`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Msi => "msi",
            Self::Nsis => "nsis",
            Self::InnoSetup => "inno_setup",
            Self::InstallShield => "installshield",
            Self::WixBurn => "wix_burn",
            Self::SevenZipSfx => "7zip_sfx",
            Self::Squirrel => "squirrel",
            Self::Unknown => "unknown",
        }
    }
    /// Documented unattended-install switches, when the family has them.
    pub const fn silent_switches(self) -> Option<&'static str> {
        match self {
            Self::Msi => Some("msiexec /i <file> /qn /norestart"),
            Self::Nsis => Some("/S"),
            Self::InnoSetup => Some("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-"),
            Self::InstallShield => Some("/s /v\"/qn\""),
            Self::WixBurn => Some("/quiet /norestart"),
            Self::SevenZipSfx => Some("-y"),
            Self::Squirrel => Some("--silent"),
            Self::Unknown => None,
        }
    }
}

/// Fingerprint of an installer binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallerFingerprint {
    /// Detected family.
    pub family: InstallerFamily,
    /// Silent switches for the family.
    pub silent_switches: Option<String>,
    /// BLAKE3 of the whole binary.
    pub binary_blake3: String,
    /// Size in bytes.
    pub size: u64,
}

const SCAN_BYTES: u64 = 8 * 1024 * 1024;

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Classify from the leading bytes and file name.
pub fn classify(head: &[u8], path: &Path) -> InstallerFamily {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if contains(head, b".wixburn") {
        InstallerFamily::WixBurn
    } else if contains(head, b"InstallShield") {
        InstallerFamily::InstallShield
    } else if contains(head, b"Inno Setup Setup Data") || contains(head, b"JR.Inno.Setup") {
        InstallerFamily::InnoSetup
    } else if contains(head, b"Nullsoft Install System") || contains(head, b"NullsoftInst") {
        InstallerFamily::Nsis
    } else if contains(head, b"Squirrel.Windows") || contains(head, b"SquirrelSetup") {
        InstallerFamily::Squirrel
    } else if contains(head, b";!@Install@!UTF-8!") {
        InstallerFamily::SevenZipSfx
    } else if head.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
        && matches!(ext.as_str(), "msi" | "msp")
    {
        InstallerFamily::Msi
    } else {
        InstallerFamily::Unknown
    }
}

/// Fingerprint an installer file (blocking; scans the first 8 MiB for markers).
pub fn fingerprint_file(path: &Path) -> Result<InstallerFingerprint> {
    let f = std::fs::File::open(path).map_err(io_err(path))?;
    let size = f.metadata().map_err(io_err(path))?.len();
    let mut head = Vec::new();
    f.take(SCAN_BYTES)
        .read_to_end(&mut head)
        .map_err(io_err(path))?;
    let family = classify(&head, path);
    Ok(InstallerFingerprint {
        family,
        silent_switches: family.silent_switches().map(str::to_string),
        binary_blake3: hash_file_blocking(path)?,
        size,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn classifies_markers() {
        let p = Path::new("setup.exe");
        assert_eq!(
            classify(b"MZ....Inno Setup Setup Data (6.2)", p),
            InstallerFamily::InnoSetup
        );
        assert_eq!(
            classify(b"MZ..Nullsoft Install System v3", p),
            InstallerFamily::Nsis
        );
        assert_eq!(classify(b"MZ..x.wixburn..", p), InstallerFamily::WixBurn);
        assert_eq!(classify(b"MZ plain", p), InstallerFamily::Unknown);
        let ole = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0];
        assert_eq!(classify(&ole, Path::new("a.msi")), InstallerFamily::Msi);
        assert_eq!(classify(&ole, Path::new("a.doc")), InstallerFamily::Unknown);
    }

    #[test]
    fn fingerprints_file_with_switches_and_hash() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("s.exe");
        std::fs::write(&f, b"MZ...NullsoftInst...").unwrap();
        let fp = fingerprint_file(&f).unwrap();
        assert_eq!(fp.family, InstallerFamily::Nsis);
        assert_eq!(fp.silent_switches.as_deref(), Some("/S"));
        assert_eq!(
            fp.binary_blake3,
            crate::hashing::hash_bytes(b"MZ...NullsoftInst...")
        );
    }
}
