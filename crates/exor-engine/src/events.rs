//! Raw kernel-visibility events and the event-source abstraction.
//!
//! Production sources (USN journal reader, registry-callback driver client) live in
//! `exor-win`. [`MockEventSource`] replays scripted or seeded-random events so the whole
//! pipeline is testable with `cargo test`, without a driver.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

/// Filesystem change reason (collapsed from the USN reason bitmask).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsReason {
    /// File or directory created.
    Create,
    /// Deleted.
    Delete,
    /// Content or attributes changed.
    Modify,
    /// Renamed (new name).
    Rename,
}

impl FsReason {
    /// Collapse a `USN_RECORD.Reason` bitmask. Precedence: delete, create, rename, modify.
    /// Returns `None` for reasons that carry no change (e.g. bare CLOSE).
    pub fn from_usn_reason(r: u32) -> Option<Self> {
        const DATA_MASK: u32 = 0x1
            | 0x2
            | 0x4
            | 0x10
            | 0x20
            | 0x40
            | 0x800
            | 0x1_0000
            | 0x2_0000
            | 0x4_0000
            | 0x20_0000;
        if r & 0x200 != 0 {
            Some(Self::Delete)
        } else if r & 0x100 != 0 {
            Some(Self::Create)
        } else if r & 0x2000 != 0 {
            Some(Self::Rename)
        } else if r & DATA_MASK != 0 {
            Some(Self::Modify)
        } else {
            None
        }
    }
}

/// Registry operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegOp {
    /// Key created.
    CreateKey,
    /// Key deleted.
    DeleteKey,
    /// Value written.
    SetValue,
    /// Value deleted.
    DeleteValue,
}

/// One observed event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawEvent {
    /// NTFS USN journal record.
    Fs {
        /// Update sequence number.
        usn: u64,
        /// Full path.
        path: String,
        /// Collapsed reason.
        reason: FsReason,
        /// Originating process image name, when known.
        process: Option<String>,
        /// Milliseconds timestamp.
        ts_ms: u64,
    },
    /// Registry callback record.
    Registry {
        /// Full key path (`HKLM\...`).
        key: String,
        /// Value name for value operations.
        value_name: Option<String>,
        /// Operation.
        op: RegOp,
        /// Originating process image name.
        process: Option<String>,
        /// Milliseconds timestamp.
        ts_ms: u64,
    },
}

impl RawEvent {
    /// Timestamp in ms.
    pub fn ts_ms(&self) -> u64 {
        match self {
            Self::Fs { ts_ms, .. } | Self::Registry { ts_ms, .. } => *ts_ms,
        }
    }
    /// Originating process, if known.
    pub fn process(&self) -> Option<&str> {
        match self {
            Self::Fs { process, .. } | Self::Registry { process, .. } => process.as_deref(),
        }
    }
}

/// A producer of [`RawEvent`]s. `start` must be called inside a tokio runtime.
pub trait EventSource: Send + 'static {
    /// Begin producing; the channel closes when the source ends.
    fn start(self: Box<Self>) -> mpsc::Receiver<RawEvent>;
}

/// Deterministic replay source for tests and development.
#[derive(Debug, Clone)]
pub struct MockEventSource {
    events: Vec<RawEvent>,
    delay: Option<Duration>,
}

impl MockEventSource {
    /// Replay exactly these events.
    pub fn scripted(events: Vec<RawEvent>) -> Self {
        Self {
            events,
            delay: None,
        }
    }

    /// Replay seeded pseudo-random traffic: an install of `app_dir` mixed with background noise.
    pub fn generated(seed: u64, count: usize, app_dir: &str) -> Self {
        Self {
            events: generate(seed, count, app_dir),
            delay: None,
        }
    }

    /// Sleep between events (simulates live traffic).
    pub fn with_delay(mut self, d: Duration) -> Self {
        self.delay = Some(d);
        self
    }
}

impl EventSource for MockEventSource {
    fn start(self: Box<Self>) -> mpsc::Receiver<RawEvent> {
        let (tx, rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            for e in self.events {
                if let Some(d) = self.delay {
                    tokio::time::sleep(d).await;
                }
                if tx.send(e).await.is_err() {
                    break;
                }
            }
        });
        rx
    }
}

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// Seeded event generator; same seed always yields the same stream.
pub fn generate(seed: u64, count: usize, app_dir: &str) -> Vec<RawEvent> {
    let mut rng = XorShift(seed.max(1));
    let mut ts = 1_000u64;
    let mut usn = 0x1000u64;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        ts += rng.below(40);
        usn += 0x60;
        let ev = match rng.below(10) {
            0..=3 => RawEvent::Fs {
                usn,
                path: format!("{app_dir}\\bin\\file{i}.dll"),
                reason: if rng.below(4) == 0 { FsReason::Modify } else { FsReason::Create },
                process: Some("setup.exe".into()),
                ts_ms: ts,
            },
            4 | 5 => RawEvent::Registry {
                key: format!("HKLM\\SOFTWARE\\Vendor\\App\\k{}", rng.below(8)),
                value_name: Some(format!("v{i}")),
                op: RegOp::SetValue,
                process: Some("setup.exe".into()),
                ts_ms: ts,
            },
            6 => RawEvent::Fs {
                usn,
                path: format!(
                    "C:\\Users\\u\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\f_{:06x}",
                    rng.below(0xFFFFFF)
                ),
                reason: FsReason::Create,
                process: Some("chrome.exe".into()),
                ts_ms: ts,
            },
            7 => RawEvent::Fs {
                usn,
                path: format!("C:\\ProgramData\\Microsoft\\Search\\Data\\Applications\\Windows\\Windows{}.edb", rng.below(3)),
                reason: FsReason::Modify,
                process: Some("SearchIndexer.exe".into()),
                ts_ms: ts,
            },
            8 => RawEvent::Fs {
                usn,
                path: format!("C:\\ProgramData\\Microsoft\\Windows Defender\\Scans\\History\\h{}", rng.below(50)),
                reason: FsReason::Create,
                process: Some("MsMpEng.exe".into()),
                ts_ms: ts,
            },
            _ => RawEvent::Registry {
                key: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\UserAssist\\{GUID}\\Count".into(),
                value_name: Some("hash".into()),
                op: RegOp::SetValue,
                process: Some("explorer.exe".into()),
                ts_ms: ts,
            },
        };
        out.push(ev);
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn usn_reason_collapse() {
        assert_eq!(
            FsReason::from_usn_reason(0x100 | 0x2),
            Some(FsReason::Create)
        );
        assert_eq!(
            FsReason::from_usn_reason(0x200 | 0x8000_0000),
            Some(FsReason::Delete)
        );
        assert_eq!(FsReason::from_usn_reason(0x2000), Some(FsReason::Rename));
        assert_eq!(FsReason::from_usn_reason(0x2), Some(FsReason::Modify));
        assert_eq!(FsReason::from_usn_reason(0x8000_0000), None);
    }

    #[test]
    fn generator_is_deterministic() {
        assert_eq!(generate(7, 50, "C:\\A"), generate(7, 50, "C:\\A"));
        assert_ne!(generate(7, 50, "C:\\A"), generate(8, 50, "C:\\A"));
    }

    #[tokio::test]
    async fn mock_source_replays_in_order() {
        let evs = generate(1, 20, "C:\\A");
        let mut rx = Box::new(MockEventSource::scripted(evs.clone())).start();
        let mut got = Vec::new();
        while let Some(e) = rx.recv().await {
            got.push(e);
        }
        assert_eq!(got, evs);
    }
}

/// Why a live lab sample can no longer be treated as complete. Not written into the change ledger
/// as if it were a file observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveEnd {
    /// Consumer requested stop; outstanding I/O was drained.
    Stopped,
    /// Record window expired or the source was reset.
    JournalGap,
    /// A bounded queue filled. A later snapshot is needed, not silent continuation.
    Backpressure,
    /// Unsupported or incomplete data was returned.
    InvalidRecord,
    /// An adapter could not read or continue the stream.
    Unavailable(String),
}

/// Experimental live sources are intentionally separate from the snapshot desktop pipeline.
/// Await the terminal result as well as consuming events; channel closure is not proof of coverage.
pub trait LiveEventSource: Send + 'static {
    /// Start bounded collection within a Tokio runtime and return the drain/terminal handle.
    fn start_live(
        self: Box<Self>,
        stop: crate::Cancel,
    ) -> (mpsc::Receiver<RawEvent>, tokio::task::JoinHandle<LiveEnd>);
}
