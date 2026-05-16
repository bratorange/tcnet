use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RawOutput {
    bpm: f64,
    beats: Vec<f64>,
    downbeats: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct DetectedBeats {
    pub bpm: f32,
    /// Sequence of `(beat_number, beat_type, timestamp_ms)`.
    /// `beat_number` is 1-based across the whole track.
    /// `beat_type` follows the TCNet convention: 20 = downbeat, 10 = upbeat.
    pub entries: Vec<(u16, u8, u32)>,
}

/// After madmom returns its detected beats, build a continuous beat grid
/// covering the entire track length using the detected BPM and the first
/// downbeat as a phase anchor. madmom often misses the first few seconds of
/// an electronic track when the pulse is ambiguous; this guarantees the VJ
/// sees a beat grid from t=0 onward, with bar boundaries phase-aligned to
/// where madmom found them.
fn synthesise_grid(
    bpm: f32,
    first_downbeat_ms: u32,
    track_length_ms: u32,
) -> Vec<(u16, u8, u32)> {
    if bpm < 20.0 || bpm > 300.0 || track_length_ms == 0 {
        return Vec::new();
    }
    let beat_interval_ms = 60_000.0 / bpm;
    let bar_interval_ms = beat_interval_ms * 4.0;
    // Phase of the bar (how far into a bar madmom's first downbeat sits).
    let phase = (first_downbeat_ms as f32) % bar_interval_ms;
    // Walk back to the first beat at or after t=0 that's phase-aligned to a
    // downbeat (i.e. lies on the same residue mod bar_interval_ms as
    // first_downbeat_ms).
    let mut t = phase;
    let mut downbeat_counter: u32 = 0;
    let mut entries = Vec::with_capacity(
        ((track_length_ms as f32) / beat_interval_ms) as usize + 8,
    );
    let mut beat_index = 0u32;
    while t < track_length_ms as f32 + 1.0 {
        let beat_type = if downbeat_counter % 4 == 0 { 20u8 } else { 10u8 };
        let beat_number = (beat_index + 1).min(u16::MAX as u32) as u16;
        entries.push((beat_number, beat_type, t.round() as u32));
        beat_index += 1;
        downbeat_counter += 1;
        t += beat_interval_ms;
    }
    entries
}

/// Run the madmom-based beat extractor. Returns `None` if the helper can't be
/// found, exits non-zero, or produces no beats. Caller falls back to a
/// constant-BPM grid in that case.
///
/// `track_length_ms` is used to extend madmom's beat output into a continuous
/// grid covering the full track, anchored to madmom's first downbeat for
/// phase. madmom often skips ambiguous-pulse intros (e.g. ambient pads) and
/// also gives up before the track end — the VJ wants beat grid coverage
/// everywhere.
pub fn detect(
    script_dir: &Path,
    audio_path: &Path,
    track_length_ms: u32,
) -> Option<DetectedBeats> {
    let script = script_dir.join("sim_beatgrid.py");
    if !script.exists() {
        log::warn!("sim_beatgrid.py not found at {:?} — beat-grid analysis disabled", script);
        return None;
    }
    let out_path = std::env::temp_dir().join(format!(
        "tcnet-sim-beatgrid-{}-{}.json",
        std::process::id(),
        audio_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("track")
    ));

    let python = crate::python_helper::resolve_python();
    let output = match Command::new(&python)
        .arg(&script)
        .arg(audio_path)
        .arg(&out_path)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            log::warn!(
                "sim_beatgrid spawn failed (python={:?}): {} — falling back to constant BPM",
                python,
                e
            );
            return None;
        }
    };
    if !output.status.success() {
        log::warn!(
            "sim_beatgrid exited {:?} (python={:?}): {}",
            output.status.code(),
            python,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }

    let parsed = parse_json(&out_path);
    let _ = std::fs::remove_file(&out_path);

    let mut parsed = match parsed {
        Some(p) => p,
        None => {
            log::warn!(
                "sim_beatgrid produced no beats for {:?} — madmom/librosa probably missing in {:?}",
                audio_path,
                python
            );
            return None;
        }
    };

    // Build a continuous grid from madmom's BPM + downbeat phase. We pick
    // madmom's first downbeat (or first beat if no downbeats) as the phase
    // anchor; everything else is synthesised at the constant BPM.
    let first_downbeat_ms = parsed
        .entries
        .iter()
        .find(|(_, ty, _)| *ty == 20)
        .map(|(_, _, ts)| *ts)
        .or_else(|| parsed.entries.first().map(|(_, _, ts)| *ts))
        .unwrap_or(0);
    let synth = synthesise_grid(parsed.bpm, first_downbeat_ms, track_length_ms);
    log::info!(
        "sim_beatgrid: madmom={} entries, synthesised continuous grid={} entries (bpm={:.2}, first downbeat anchor={}ms)",
        parsed.entries.len(),
        synth.len(),
        parsed.bpm,
        first_downbeat_ms
    );
    parsed.entries = synth;

    Some(parsed)
}

fn parse_json(path: &PathBuf) -> Option<DetectedBeats> {
    let text = std::fs::read_to_string(path).ok()?;
    let raw: RawOutput = serde_json::from_str(&text).ok()?;
    if raw.beats.is_empty() {
        return None;
    }

    let downbeats: std::collections::HashSet<u32> = raw
        .downbeats
        .iter()
        .map(|d| (d * 1000.0).round() as u32)
        .collect();

    let entries: Vec<(u16, u8, u32)> = raw
        .beats
        .iter()
        .enumerate()
        .map(|(i, &t_sec)| {
            let ts_ms = (t_sec * 1000.0).round() as u32;
            let beat_type = if downbeats.contains(&ts_ms) {
                20u8
            } else {
                10u8
            };
            ((i + 1) as u16, beat_type, ts_ms)
        })
        .collect();

    Some(DetectedBeats {
        bpm: raw.bpm as f32,
        entries,
    })
}
