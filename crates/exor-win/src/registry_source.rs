//! Client for the registry-callback minifilter driver (`driver/exordrv.c`).
//!
//! Connects to the driver's communication port and forwards parsed messages. The blocking
//! `FilterGetMessage` runs on the tokio blocking pool; after the consumer drops its receiver the
//! thread exits at the next message or when the driver unloads.

use crate::{WinError, proto};
use exor_engine::events::{EventSource, RawEvent};
use tokio::sync::mpsc;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::InstallableFileSystems::{
    FILTER_MESSAGE_HEADER, FilterConnectCommunicationPort, FilterGetMessage,
};
use windows::core::HSTRING;

struct Port(HANDLE);
impl Drop for Port {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful FilterConnectCommunicationPort and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Registry events delivered by the kernel driver.
#[derive(Debug, Default, Clone, Copy)]
pub struct DriverRegistrySource;

impl DriverRegistrySource {
    /// Connect to the driver port (administrator rights required) to verify the driver is loaded.
    pub fn probe() -> Result<(), WinError> {
        connect().map(|_| ())
    }
}

fn connect() -> Result<Port, WinError> {
    let name = HSTRING::from(proto::PORT_NAME);
    // SAFETY: `name` is a valid wide string for the duration of the call; no context or security attributes are passed.
    let h = unsafe { FilterConnectCommunicationPort(&name, 0, None, 0, None) }.map_err(|e| {
        WinError::Win32 {
            call: "FilterConnectCommunicationPort",
            message: e.message(),
            code: e.code().0 as u32,
        }
    })?;
    Ok(Port(h))
}

fn run(tx: &mpsc::Sender<RawEvent>) -> Result<(), WinError> {
    let port = connect()?;
    let header = std::mem::size_of::<FILTER_MESSAGE_HEADER>();
    // u64 backing keeps the FILTER_MESSAGE_HEADER at the 8-byte alignment it requires.
    let mut storage = vec![0u64; 16 * 1024];
    let size_bytes = storage.len() * 8;
    while !tx.is_closed() {
        // SAFETY: `storage` is a live, 8-byte aligned buffer of `size_bytes` bytes, large enough for the header plus payload.
        unsafe {
            FilterGetMessage(
                port.0,
                storage.as_mut_ptr().cast::<FILTER_MESSAGE_HEADER>(),
                size_bytes as u32,
                None,
            )
        }
        .map_err(|e| WinError::Win32 {
            call: "FilterGetMessage",
            message: e.message(),
            code: e.code().0 as u32,
        })?;
        // SAFETY: reinterpreting initialised u64 storage as bytes is always valid; length is exact.
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), size_bytes) };
        if let Ok(ev) = proto::parse_registry_message(&bytes[header..])
            && tx.blocking_send(ev).is_err()
        {
            return Ok(());
        }
    }
    Ok(())
}

impl EventSource for DriverRegistrySource {
    fn start(self: Box<Self>) -> mpsc::Receiver<RawEvent> {
        let (tx, rx) = mpsc::channel(4096);
        tokio::task::spawn_blocking(move || {
            // On failure the channel closes; call `DriverRegistrySource::probe` first to learn why.
            let _ = run(&tx);
        });
        rx
    }
}
