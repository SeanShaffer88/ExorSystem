//! BLAKE3 file and byte hashing.

use crate::error::{Result, io_err};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Hex BLAKE3 of a byte slice.
pub fn hash_bytes(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// Streaming hex BLAKE3 of a file (blocking).
pub fn hash_file_blocking(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path).map_err(io_err(path))?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(io_err(path))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_hex().to_string())
}

/// Hash a file on the blocking pool so the async runtime stays responsive.
pub async fn hash_file(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || hash_file_blocking(&path)).await?
}
