//! Windows-specific event sources and read-only system probes. Event parsing is portable;
//! Win32 I/O compiles on Windows only.

pub mod proto;
pub mod usn;

#[cfg(any(windows, test))]
mod activity_layout;
#[cfg(windows)]
mod activity_window;
#[cfg(windows)]
mod desktop;
#[cfg(windows)]
mod environment_source;
#[cfg(windows)]
mod locks;
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
mod trust;
#[cfg(windows)]
mod usn_source;

#[cfg(windows)]
pub use activity_window::{place_activity_window, show_activity_window};
#[cfg(windows)]
pub use desktop::{
    pick_path, prepare_user_data_dir, process_is_elevated, show_startup_error, uninstall_arguments,
    uninstall_executable,
};
#[cfg(windows)]
pub use environment_source::{expand as expand_environment, stored_variables};
#[cfg(windows)]
pub use locks::locking_apps;
#[cfg(windows)]
pub use packages_source::enumerate_packages;
#[cfg(windows)]
pub use programs_source::{enumerate_programs, enumerate_software_keys, registry_program};
#[cfg(windows)]
pub use registry_snapshot::{capture_registry_cancellable, capture_registry_keys};
#[cfg(windows)]
pub use registry_source::DriverRegistrySource;
#[cfg(windows)]
pub use services_source::{enumerate_services, enumerate_services_cancellable};
#[cfg(windows)]
pub use trust::verify_signature;
#[cfg(windows)]
pub use usn_source::UsnJournalSource;

/// Convert the Windows epoch (100 ns ticks since 1601) to Unix milliseconds.
fn filetime_to_unix_ms(ticks: u64) -> u64 {
    ticks.saturating_sub(116_444_736_000_000_000) / 10_000
}

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
    /// Scope data could not be captured safely.
    #[error("{0}")]
    Coverage(&'static str),
    /// Malformed driver message.
    #[error("malformed driver message: {0}")]
    Proto(&'static str),
}
