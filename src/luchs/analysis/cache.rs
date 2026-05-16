use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Returns the root cache directory LUCHS uses. Created on demand by callers.
pub fn cache_root() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("luchs")
}

/// Stable cache key for a media file. Combines absolute path + file size so a
/// file replaced in place with new audio gets a new key. Hex of the SHA-256
/// prefix (16 bytes) — short enough for directory names but ~unique.
pub fn key_for_file(path: &Path) -> String {
    let size = std::fs::metadata(path)
        .map(|m| m.len())
        .unwrap_or(0);
    let path_str = path.to_string_lossy();
    let mut h = Sha256::new();
    h.update(path_str.as_bytes());
    h.update(&size.to_le_bytes());
    let digest = h.finalize();
    let mut s = String::with_capacity(32);
    for b in &digest[..16] {
        use std::fmt::Write;
        let _ = write!(&mut s, "{:02x}", b);
    }
    s
}

pub fn dir_for(key: &str) -> PathBuf {
    cache_root().join(key)
}

pub fn ensure_dir(key: &str) -> std::io::Result<PathBuf> {
    let p = dir_for(key);
    std::fs::create_dir_all(&p)?;
    Ok(p)
}
