use std::path::{Path, PathBuf};
use std::process::Command;
use log::info;
use serde::Deserialize;

use crate::luchs::phrase_types::{MpCurve, PitchContour};

#[derive(Debug, Deserialize)]
struct RawMpPoint {
    t: f64,
    m: f64,
}

#[derive(Debug, Deserialize)]
struct RawPitchPoint {
    t: f64,
    f0: f64,
    v: i32,
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    mp: Vec<RawMpPoint>,
    pitch: Vec<RawPitchPoint>,
}

#[derive(Debug)]
pub struct MpPitchResult {
    pub mp: MpCurve,
    pub pitch: PitchContour,
}

#[derive(Debug)]
pub struct MpPitchError(pub String);

/// Run the melodic/percussive + PitchMelodia helper script.
/// Caches `<cache_dir>/mp_pitch.json` between runs.
pub fn run(
    script: &Path,
    audio_path: &Path,
    cache_dir: &Path,
) -> Result<MpPitchResult, MpPitchError> {
    let json_path = cache_dir.join("mp_pitch.json");

    if !json_path.exists() {
        std::fs::create_dir_all(cache_dir)
            .map_err(|e| MpPitchError(format!("cache mkdir: {}", e)))?;
        let python = crate::python_helper::resolve_python();
        let output = Command::new(&python)
            .arg(script)
            .arg(audio_path)
            .arg(&json_path)
            .output()
            .map_err(|e| MpPitchError(format!("spawn {}: {}", python, e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(MpPitchError(format!(
                "mp_pitch helper exited {:?}: {}",
                output.status.code(),
                stderr.trim()
            )));
        }
    } else { 
        info!("Using cached mp_pitch output: {}", json_path.display());
    }

    parse_json(&json_path)
}

fn parse_json(path: &PathBuf) -> Result<MpPitchResult, MpPitchError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| MpPitchError(format!("read {}: {}", path.display(), e)))?;
    let raw: RawOutput = serde_json::from_str(&text)
        .map_err(|e| MpPitchError(format!("parse {}: {}", path.display(), e)))?;

    let mut mp = MpCurve::default();
    for p in raw.mp {
        mp.times_ms.push((p.t * 1000.0).max(0.0) as u32);
        mp.melodic_ratio.push(p.m.clamp(0.0, 1.0) as f32);
    }

    let mut pitch = PitchContour::default();
    for p in raw.pitch {
        pitch.times_ms.push((p.t * 1000.0).max(0.0) as u32);
        pitch.f0_hz.push(p.f0 as f32);
        pitch.voiced.push(p.v != 0);
    }

    Ok(MpPitchResult { mp, pitch })
}
