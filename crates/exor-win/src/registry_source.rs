//! Client for the registry-callback minifilter driver (`driver/exordrv.c`).
//!
//! Only used in the lab. Receive uses an overlapped operation drained on stop; queue overflow
//! terminates the sample explicitly rather than treating a lossy stream as complete.

use crate::{WinError, proto};
use exor_engine::Cancel;
use exor_engine::events::{LiveEnd, LiveEventSource, RawEvent};
use tokio::sync::mpsc;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::InstallableFileSystems::{
    FILTER_MESSAGE_HEADER, FilterConnectCommunicationPort, FilterGetMessage,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject};
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

fn werr(call: &'static str, e: windows::core::Error) -> WinError {
    WinError::Win32 {
        call,
        code: e.code().0 as u32,
        message: e.message(),
    }
}

fn run(tx: &mpsc::Sender<RawEvent>, stop: &Cancel) -> Result<LiveEnd, WinError> {
    let port = connect()?;
    // SAFETY: no foreign pointer is supplied; a manual-reset event is owned by this worker.
    let event = unsafe { CreateEventW(None, true, false, None) }
        .map(Port)
        .map_err(|e| werr("CreateEventW", e))?;
    let header = std::mem::size_of::<FILTER_MESSAGE_HEADER>();
    let mut storage = vec![0u64; 16 * 1024];
    let capacity = storage.len() * 8;
    while !tx.is_closed() && !stop.is_cancelled() {
        // SAFETY: the live event is used only by this worker, with no outstanding I/O.
        unsafe { ResetEvent(event.0) }.map_err(|e| werr("ResetEvent", e))?;
        let mut ov = OVERLAPPED {
            hEvent: event.0,
            ..Default::default()
        };
        // SAFETY: `storage` is aligned and kept live along with `ov` until I/O is drained.
        let started = unsafe {
            FilterGetMessage(
                port.0,
                storage.as_mut_ptr().cast(),
                capacity as u32,
                Some(&mut ov),
            )
        };
        let pending = match started {
            Ok(()) => false,
            Err(e) if e.code().0 as u32 == 0x8007_03e5 => true,
            Err(e) => return Err(werr("FilterGetMessage", e)),
        };
        let mut used = 0u32;
        if pending {
            loop {
                // SAFETY: a live event handle; timeout limits time until checking stop.
                let wait = unsafe { WaitForSingleObject(event.0, 250) };
                if wait == WAIT_OBJECT_0 {
                    // SAFETY: the OVERLAPPED belongs to this port, and the byte-count out pointer is live.
                    unsafe { GetOverlappedResult(port.0, &ov, &mut used, true) }
                        .map_err(|e| werr("GetOverlappedResult", e))?;
                    break;
                }
                if wait != WAIT_TIMEOUT || tx.is_closed() || stop.is_cancelled() {
                    // SAFETY: cancel and drain precisely this operation before releasing its buffers.
                    unsafe {
                        let _ = CancelIoEx(port.0, Some(&ov));
                        let _ = GetOverlappedResult(port.0, &ov, &mut used, true);
                    }
                    return Ok(if wait == WAIT_TIMEOUT {
                        LiveEnd::Stopped
                    } else {
                        LiveEnd::Unavailable("Waiting for the registry port failed".into())
                    });
                }
            }
        } else {
            // SAFETY: synchronous completion still stores its byte count in this OVERLAPPED.
            unsafe { GetOverlappedResult(port.0, &ov, &mut used, false) }
                .map_err(|e| werr("GetOverlappedResult", e))?;
        }
        if used as usize <= header || used as usize > capacity {
            return Ok(LiveEnd::InvalidRecord);
        }
        // SAFETY: I/O is complete; `used` cannot exceed the initialized backing allocation.
        let bytes =
            unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), used as usize) };
        let Ok(event) = proto::parse_registry_message(&bytes[header..]) else {
            return Ok(LiveEnd::InvalidRecord);
        };
        match tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => return Ok(LiveEnd::Stopped),
            Err(mpsc::error::TrySendError::Full(_)) => return Ok(LiveEnd::Backpressure),
        }
    }
    Ok(LiveEnd::Stopped)
}

impl LiveEventSource for DriverRegistrySource {
    fn start_live(
        self: Box<Self>,
        stop: Cancel,
    ) -> (mpsc::Receiver<RawEvent>, tokio::task::JoinHandle<LiveEnd>) {
        let (tx, rx) = mpsc::channel(4096);
        let done = tokio::task::spawn_blocking(move || {
            run(&tx, &stop).unwrap_or_else(|e| LiveEnd::Unavailable(e.to_string()))
        });
        (rx, done)
    }
}
