use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub duration_ms: u32,
    pub bpm: Option<f32>,
}

pub struct VirtualUsb {
    pub root: PathBuf,
    pub tracks: Vec<TrackInfo>,
}

impl VirtualUsb {
    pub fn from_dir(root: PathBuf) -> Self {
        let mut usb = Self { root, tracks: Vec::new() };
        usb.scan();
        usb
    }

    pub fn scan(&mut self) {
        self.tracks.clear();
        if !self.root.exists() { return; }
        self.scan_dir(&self.root.clone());
        self.tracks.sort_by(|a, b| a.title.cmp(&b.title));
    }

    fn scan_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir(&path);
            } else if is_audio_file(&path) {
                if let Some(info) = read_track_info(path) {
                    self.tracks.push(info);
                }
            }
        }
    }
}

fn is_audio_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref(),
        Some("mp3" | "flac" | "wav" | "aac" | "m4a" | "ogg")
    )
}

fn read_track_info(path: PathBuf) -> Option<TrackInfo> {
    use lofty::prelude::*;
    use lofty::probe::Probe;

    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_owned();

    let result = Probe::open(&path).ok().and_then(|p| p.read().ok());

    let (title, artist, duration_ms, bpm) = if let Some(tagged) = result {
        let props = tagged.properties();
        let duration_ms = props.duration().as_millis() as u32;
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
        let (title, artist, bpm) = if let Some(tag) = tag {
            let title = tag.title().map(|s| s.to_string()).unwrap_or_else(|| stem.clone());
            let artist = tag.artist().map(|s| s.to_string()).unwrap_or_default();
            let bpm = tag.get_string(&lofty::tag::ItemKey::Bpm)
                .and_then(|s| s.parse::<f32>().ok());
            (title, artist, bpm)
        } else {
            (stem, String::new(), None)
        };
        (title, artist, duration_ms, bpm)
    } else {
        (stem, String::new(), 0, None)
    };

    Some(TrackInfo { path, title, artist, duration_ms, bpm })
}
