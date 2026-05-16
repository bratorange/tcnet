use std::path::{Path, PathBuf};

const EXTENSIONS: &[&str] = &["wav", "flac", "mp3", "aac", "m4a", "ogg"];

/// Resolve a track title (as published over TCNet) to an audio file path in
/// `media_dir`. MVP heuristic: case-insensitive substring match on filename
/// stems, restricted to the audio extensions LUCHS supports.
pub fn resolve(media_dir: &Path, title: &str) -> Option<PathBuf> {
    if title.is_empty() {
        return None;
    }
    let needle = title.trim().to_ascii_lowercase();

    let entries = walk(media_dir);
    // Prefer exact-stem match; fall back to substring.
    let mut best: Option<PathBuf> = None;
    for path in &entries {
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_ascii_lowercase(),
            None => continue,
        };
        if stem == needle {
            return Some(path.clone());
        }
        if stem.contains(&needle) && best.is_none() {
            best = Some(path.clone());
        }
    }
    best
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_inner(dir, &mut out, 0);
    out
}

fn walk_inner(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_inner(&p, out, depth + 1);
        } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                out.push(p);
            }
        }
    }
}
