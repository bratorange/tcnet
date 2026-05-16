use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rodio::{Decoder, Source};

/// Output of `compute_waveforms` — TCNet wire-format bytes for both packets.
pub struct GeneratedWaveform {
    pub small: [u8; 2400],
    pub big: Vec<u8>,
}

const SMALL_COLUMNS: usize = 1200;
const BIG_COLUMNS: usize = 5000;

/// Decode the file at `path` into mono f32 samples and compute small (1200 col)
/// and big (5000 col) TCNet-style waveform bytes.
///
/// Each output column is two bytes: `[level, color]`. Level is `(rms * 255)`
/// clamped to 0-255; color is a placeholder band byte derived from level
/// (low → blue 0x03, mid → green 0x04, high → orange 0x05). A real CDJ bridge
/// would encode spectral-band dominance here; we'll plug in proper colours
/// when phrase-tinted rendering lands in Phase 4.
pub fn compute_waveforms(path: &Path) -> Option<GeneratedWaveform> {
    let mono = decode_mono_f32(path)?;
    if mono.is_empty() {
        return None;
    }
    let small_bytes = compute_columns(&mono, SMALL_COLUMNS);
    let mut small = [0u8; 2400];
    small.copy_from_slice(&small_bytes);

    let big = compute_columns(&mono, BIG_COLUMNS);
    Some(GeneratedWaveform { small, big })
}

/// Decode the file to a mono f32 sample buffer. Returns None on decode error.
fn decode_mono_f32(path: &Path) -> Option<Vec<f32>> {
    let file = File::open(path).ok()?;
    let decoder = Decoder::new(BufReader::new(file)).ok()?;
    let channels = decoder.channels() as usize;
    if channels == 0 {
        return None;
    }
    let samples_i16: Vec<i16> = decoder.collect();
    let mut mono = Vec::with_capacity(samples_i16.len() / channels);
    for frame in samples_i16.chunks_exact(channels) {
        let sum: f32 = frame.iter().map(|&s| s as f32).sum();
        mono.push(sum / (channels as f32 * i16::MAX as f32));
    }
    Some(mono)
}

/// Split `samples` into `cols` RMS windows and pack into `[level, color]` pairs.
fn compute_columns(samples: &[f32], cols: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(cols * 2);
    let window = (samples.len() / cols).max(1);
    for i in 0..cols {
        let start = i * window;
        let end = (start + window).min(samples.len());
        let level = if end > start {
            let slice = &samples[start..end];
            let sum_sq: f32 = slice.iter().map(|s| s * s).sum();
            let rms = (sum_sq / slice.len() as f32).sqrt();
            (rms * 2.5 * 255.0).clamp(0.0, 255.0) as u8
        } else {
            0
        };
        let color = level_to_band(level);
        out.push(level);
        out.push(color);
    }
    out
}

fn level_to_band(level: u8) -> u8 {
    if level < 0x40 {
        0x03
    } else if level < 0xA0 {
        0x04
    } else {
        0x05
    }
}
