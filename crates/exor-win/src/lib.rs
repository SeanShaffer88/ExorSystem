//! Windows-specific event sources. Parsing logic is portable and unit-tested everywhere;
//! the Win32 I/O (`usn_source`, `registry_source`) compiles on Windows only.

pub mod proto;
pub mod usn;

#[cfg(windows)]
mod registry_source;
#[cfg(windows)]
mod usn_source;

#[cfg(windows)]
pub use registry_source::DriverRegistrySource;
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
