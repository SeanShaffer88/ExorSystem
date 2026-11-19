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
    hash_reader(&mut f, path, None)
}

pub(crate) fn hash_reader(
    input: &mut impl Read,
    path: &Path,
    cancel: Option<&crate::Cancel>,
) -> Result<String> {
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        if let Some(cancel) = cancel {
            cancel.check()?;
        }
        let n = input.read(&mut buf).map_err(io_err(path))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_hex().to_string())
}

pub(crate) fn hash_capture(
    path: &Path,
    size: u64,
    mtime_ns: i64,
    cancel: &crate::Cancel,
    bytes: &std::sync::atomic::AtomicU64,
) -> Result<String> {
    use std::sync::atomic::Ordering::Relaxed;
    cancel.check()?;
    let matches = |meta: &std::fs::Metadata| {
        meta.is_file()
            && !crate::walker::redirected(meta)
            && meta.len() == size
            && crate::walker::mtime_ns(meta) == mtime_ns
    };
    let md = std::fs::symlink_metadata(path).map_err(io_err(path))?;
    if !matches(&md) {
        return Err(crate::EngineError::Watch(
            "file changed after the directory walk".into(),
        ));
    }
    if let Some(parent) = path.parent()
        && !crate::quarantine::plain_dirs(parent)?
    {
        return Err(crate::EngineError::Watch("file parent disappeared".into()));
    }
    #[cfg(windows)]
    let _parents = exor_fs::pin_parents(path.parent().unwrap_or(path)).map_err(io_err(path))?;
    #[cfg(windows)]
    let input = exor_fs::open_locked(path, false).map_err(io_err(path))?;
    #[cfg(not(windows))]
    let input = std::fs::File::open(path).map_err(io_err(path))?;
    if !matches(&input.metadata().map_err(io_err(path))?) {
        return Err(crate::EngineError::Watch("file changed during open".into()));
    }
    let mut reader = (&input).take(size.saturating_add(1));
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0_u8; 128 * 1024];
    let mut total = 0;
    loop {
        cancel.check()?;
        let n = reader.read(&mut buf).map_err(io_err(path))?;
        if n == 0 {
            break;
        }
        bytes.fetch_add(n as u64, Relaxed);
        total += n as u64;
        h.update(&buf[..n]);
    }
    if total != size || !matches(&input.metadata().map_err(io_err(path))?) {
        return Err(crate::EngineError::Watch(
            "file changed during hashing".into(),
        ));
    }
    Ok(h.finalize().to_hex().to_string())
}

/// Hash a file on the blocking pool so the async runtime stays responsive.
pub async fn hash_file(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || hash_file_blocking(&path)).await?
}
