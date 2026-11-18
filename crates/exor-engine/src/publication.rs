//! Publication of complete exports. Temporary work is isolated in a directory we created.

use crate::error::{EngineError, Result, io_err};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn sync_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(io_err(dir))?;
    // The Windows desktop uses the same local filesystem for a quarantine move. Rust's
    // portable File API does not offer a Windows directory/volume metadata flush guarantee.
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

fn nonce() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{time:x}-{:x}-{:x}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn component(name: &str) -> Result<()> {
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\\', ':', '\0']) {
        return Err(EngineError::Quarantine("invalid export name".into()));
    }
    Ok(())
}

/// A same-volume workspace removed on ordinary failure. A killed process can leave an
/// identifiable `.exor-incomplete-*` directory, never a published partial file.
pub struct Staging(PathBuf);

impl Staging {
    /// Create a fresh workspace inside an existing destination directory.
    pub fn new(parent: &Path) -> Result<Self> {
        let path = parent.join(format!(".exor-incomplete-{}", nonce()));
        fs::create_dir(&path).map_err(io_err(&path))?;
        Ok(Self(path))
    }

    /// Directory to fill before publication.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Publish a staged directory under a unique new name. The desktop targets Windows,
    /// where rename cannot replace an existing directory. The unique suffix also avoids
    /// accidental collisions on non-Windows development hosts.
    pub fn publish_directory(self, label: &str) -> Result<PathBuf> {
        component(label)?;
        let parent = self
            .0
            .parent()
            .ok_or_else(|| EngineError::Quarantine("missing output directory".into()))?;
        let path = parent.join(format!("{label} - {}", nonce()));
        self.publish_as(&path)?;
        Ok(path)
    }

    /// Commit to a new destination in the same parent directory.
    pub fn publish_as(self, path: &Path) -> Result<()> {
        let parent = self
            .0
            .parent()
            .ok_or_else(|| EngineError::Quarantine("missing output directory".into()))?;
        if path.parent() != Some(parent) || path.symlink_metadata().is_ok() {
            return Err(io_err(path)(std::io::Error::from(ErrorKind::AlreadyExists)));
        }
        sync_dir(&self.0)?;
        #[cfg(windows)]
        exor_fs::publish(&self.0, path, false).map_err(io_err(path))?;
        #[cfg(not(windows))]
        fs::rename(&self.0, path).map_err(io_err(path))?;
        sync_dir(parent)
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Flush a file before giving it a final filename. Existing files (including redirects) are
/// never overwritten, even if another process publishes between attempts.
pub fn write_unique(dir: &Path, stem: &str, extension: &str, bytes: &[u8]) -> Result<PathBuf> {
    component(stem)?;
    component(extension)?;
    let stage = Staging::new(dir)?;
    let temp = stage.path().join("output");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(io_err(&temp))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(io_err(&temp))?;
    drop(file);
    for n in 1..=1000 {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" ({n})")
        };
        let path = dir.join(format!("{stem}{suffix}.{extension}"));
        #[cfg(windows)]
        let published = exor_fs::publish(&temp, &path, false);
        #[cfg(not(windows))]
        let published = fs::hard_link(&temp, &path);
        match published {
            Ok(()) => {
                sync_dir(dir)?;
                return Ok(path);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io_err(&path)(e)),
        }
    }
    Err(EngineError::Quarantine(
        "too many existing exports; choose a different destination".into(),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn unique_publication_does_not_replace_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("report.md"), b"original").unwrap();
        let next = write_unique(dir.path(), "report", "md", b"complete").unwrap();
        assert_eq!(next.file_name().unwrap(), "report (2).md");
        assert_eq!(fs::read(next).unwrap(), b"complete");
        assert_eq!(fs::read(dir.path().join("report.md")).unwrap(), b"original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
