//! Audio files on disk. Clips are served to the player from here, and repeated
//! requests (listening to a word again and again) cost nothing.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::{Audio, AudioFormat, Result};

pub struct AudioCache {
    dir: PathBuf,
}

/// Stable cache key of the given parts.
pub fn key(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0u8]);
    }
    h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

impl AudioCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// File name of a cached clip, if there is one.
    pub fn find(&self, key: &str) -> Option<String> {
        AudioFormat::ALL
            .iter()
            .map(|f| format!("{key}.{}", f.extension()))
            .find(|name| self.dir.join(name).is_file())
    }

    pub fn put(&self, key: &str, audio: &Audio) -> Result<String> {
        std::fs::create_dir_all(&self.dir)?;
        let name = format!("{key}.{}", audio.format.extension());
        let tmp = self.dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, &audio.bytes)?;
        std::fs::rename(&tmp, self.dir.join(&name))?;
        Ok(name)
    }

    /// Read a clip by file name (as returned by `find` / `put`).
    pub fn read(&self, name: &str) -> Option<(Vec<u8>, AudioFormat)> {
        // Only plain file names: the name comes from a URL.
        if name.contains(['/', '\\']) || name.starts_with('.') {
            return None;
        }
        let format = AudioFormat::from_extension(Path::new(name).extension()?.to_str()?)?;
        let bytes = std::fs::read(self.dir.join(name)).ok()?;
        Some((bytes, format))
    }

    pub fn clear(&self) -> Result<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// Delete the least recently written files above `max_bytes`.
    pub fn prune(&self, max_bytes: u64) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return };
        let mut files: Vec<(SystemTime, u64, PathBuf)> = entries
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                m.is_file().then(|| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), e.path()))
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        if total <= max_bytes {
            return;
        }
        files.sort_by_key(|f| f.0);
        for (_, len, path) in files {
            if total <= max_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}
