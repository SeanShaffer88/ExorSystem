//! Read-only MSIX/AppX inventory for the current Windows user.

use crate::WinError;
use exor_engine::programs::{InstalledProgram, ProgramSource, Scope};
use std::collections::BTreeMap;
use windows::ApplicationModel::PackageSignatureKind;
use windows::Management::Deployment::{PackageManager, PackageTypes};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use windows::core::{Error, HSTRING};

struct Runtime;

impl Runtime {
    fn init() -> Result<Self, Error> {
        // SAFETY: this initialises WinRT on the current thread and is balanced by Drop here.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
        Ok(Self)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after successful RoInitialize, on this same thread.
        unsafe { RoUninitialize() };
    }
}

fn text(value: Result<HSTRING, Error>) -> Option<String> {
    value
        .ok()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
}

fn packages() -> Result<Vec<InstalledProgram>, Error> {
    let _runtime = Runtime::init()?;
    let manager = PackageManager::new()?;
    // An empty SID means the current user; don't expand this into an all-users query.
    let entries = manager
        .FindPackagesByUserSecurityIdWithPackageTypes(&HSTRING::new(), PackageTypes::Main)?;
    let iter = entries.First()?;
    let mut by_family = BTreeMap::new();
    while iter.HasCurrent()? {
        let p = iter.Current()?;
        let id = p.Id()?;
        let family = id.FamilyName()?.to_string();
        let v = id.Version()?;
        let version = (v.Major, v.Minor, v.Build, v.Revision);
        if by_family
            .get(&family)
            .is_none_or(|(previous, _)| version > *previous)
        {
            let row = InstalledProgram {
                id: format!("msix:{family}"),
                source: ProgramSource::Msix,
                name: text(p.DisplayName())
                    .or_else(|| text(id.Name()))
                    .unwrap_or_else(|| family.clone()),
                version: Some(format!(
                    "{}.{}.{}.{}",
                    v.Major, v.Minor, v.Build, v.Revision
                )),
                publisher: text(p.PublisherDisplayName()).or_else(|| text(id.Publisher())),
                install_location: text(p.InstalledPath()),
                uninstall_command: None,
                quiet_uninstall_command: None,
                install_date: None,
                size_kb: None,
                scope: Scope::User,
                system_component: p
                    .SignatureKind()
                    .is_ok_and(|kind| kind == PackageSignatureKind::System),
                parent_key: None,
                windows_installer: false,
            };
            by_family.insert(family, (version, row));
        }
        iter.MoveNext()?;
    }
    Ok(by_family.into_values().map(|(_, p)| p).collect())
}

/// Main packages that Windows has registered for the calling user. Blocking.
pub fn enumerate_packages() -> Result<Vec<InstalledProgram>, WinError> {
    packages().map_err(|e| WinError::Win32 {
        call: "Windows package inventory",
        message: e.message(),
        code: e.code().0 as u32,
    })
}
