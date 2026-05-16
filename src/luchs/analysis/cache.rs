use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// Returns the root cache directory LUCHS uses. Created on demand by callers.
pub fn cache_root() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("luchs")
}

/// Stable cache key for a track, derived from its broadcast metadata (title +
/// artist). Two files with the same title+artist tags share a cache entry —
/// which is exactly what we want: a track moved between folders or re-encoded
/// at the same metadata reuses its prior analysis output.
///
/// The key is normalised (trim, ASCII-lowercase) so cosmetic differences
/// don't fragment the cache. Hex of the SHA-256 prefix (16 bytes).
pub fn key_for_track(title: &str, artist: &str) -> String {
    let mut h = Sha256::new();
    h.update(title.trim().to_ascii_lowercase().as_bytes());
    h.update(b"\x1f"); // unit separator — disambiguates "ab|" vs "a|b"
    h.update(artist.trim().to_ascii_lowercase().as_bytes());
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
