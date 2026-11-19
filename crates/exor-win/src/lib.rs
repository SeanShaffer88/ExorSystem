//! Windows-specific event sources. Parsing logic is portable and unit-tested everywhere;
//! the Win32 I/O (`usn_source`, `registry_source`) compiles on Windows only.

pub mod proto;
pub mod usn;

#[cfg(windows)]
mod programs_source;
#[cfg(windows)]
mod registry_source;
#[cfg(windows)]
mod services_source;
#[cfg(windows)]
mod unelevated;
#[cfg(windows)]
mod usn_source;

#[cfg(windows)]
pub use unelevated::run_unelevated;

#[cfg(windows)]
pub use programs_source::enumerate_programs;
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
    /// An action was refused for safety, with the reason in plain words.
    #[error("{0}")]
    Refused(String),
}
