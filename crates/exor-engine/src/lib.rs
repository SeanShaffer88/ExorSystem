//! ExorSystem user-space engine. 100% safe Rust; platform sources live in `exor-win`.

mod cancel;
mod error;

pub mod events;
pub mod fingerprint;
pub mod hashing;
pub mod jsonlogic;
pub mod monitor;
pub mod noise;
pub mod policy;
pub mod portable;
pub mod quarantine;
pub mod report;
pub mod risk;
pub mod snapshot;
pub mod templates;
pub mod trie;
pub mod walker;

pub use cancel::Cancel;
pub use error::{EngineError, Result};
