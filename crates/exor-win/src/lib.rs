//! Windows-specific event sources and read-only system probes. Event parsing is portable;
//! Win32 I/O compiles on Windows only.

pub mod proto;
pub mod usn;

#[cfg(windows)]
mod packages_source;
#[cfg(windows)]
mod programs_source;
#[cfg(windows)]
mod registry_snapshot;
#[cfg(windows)]
mod registry_source;
#[cfg(windows)]
mod services_source;
#[cfg(windows)]
mod usn_source;

#[cfg(windows)]
pub use packages_source::enumerate_packages;
#[cfg(windows)]
pub use programs_source::{enumerate_programs, enumerate_software_keys};
#[cfg(windows)]
pub use registry_snapshot::capture_registry_keys;
#[cfg(windows)]
pub use registry_source::DriverRegistrySource;
#[cfg(windows)]
pub use services_source::enumerate_services;
#[cfg(windows)]
pub use usn_source::UsnJournalSource;

/// Errors from the Windows sources.
#[derive(Debug, thiserror::Error)]
pub enum WinError {
    /// A Win32 call failed.
    #[error("{call} failed: {message} (0x{code:08X})")]
    Win32 {
        /// API name.
        call: &'static str,
        /// OS message.
        message: String,
        /// HRESULT.
        code: u32,
    },
    /// Malformed driver message.
    #[error("malformed driver message: {0}")]
    Proto(&'static str),
}
