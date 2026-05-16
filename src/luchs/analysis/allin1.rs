use std::path::{Path, PathBuf};
use std::process::Command;
use log::info;
use serde::Deserialize;

use crate::luchs::phrase_types::{Phrase, Segment};

#[derive(Debug, Deserialize)]
struct RawSegment {
    start: f64,
    end: f64,
    label: String,
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    segments: Vec<RawSegment>,
}

#[derive(Debug)]
pub struct AnalysisResult {
    pub segments: Vec<Segment>,
}

#[derive(Debug)]
pub struct AnalysisError(pub String);

/// Run the phrase-analysis helper script for `audio_path`, cache the resulting
/// JSON in `cache_dir`, and parse it into `Segment`s in LUCHS phrase buckets.
///
/// The helper (`scripts/luchs_allin1.py`) wraps the real `allin1` CLI but
/// falls back to a deterministic stub keyed by file size so the test harness
/// runs without a real install.
pub fn run(
    script: &Path,
    audio_path: &Path,
    cache_dir: &Path,
) -> Result<AnalysisResult, AnalysisError> {
    let json_path = cache_dir.join("struct.json");

    if !json_path.exists() {
        std::fs::create_dir_all(cache_dir)
            .map_err(|e| AnalysisError(format!("cache mkdir: {}", e)))?;
        let python = crate::python_helper::resolve_python();
        let mut command = Command::new(&python);
        command
            .arg(script)
            .arg(audio_path)
            .arg(&json_path);
        info!("Running allin1 helper with script:");
        info!("{:?}", command);
        let output = command
            .output()
            .map_err(|e| AnalysisError(format!("spawn {}: {}", python, e)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(AnalysisError(format!(
                "allin1 helper exited {:?}: {}",
                output.status.code(),
                stderr.trim()
            )));
        }
    } else { 
        info!("Using cached allin1 output: {}", json_path.display());
    }

    parse_json(&json_path)
}

fn parse_json(path: &PathBuf) -> Result<AnalysisResult, AnalysisError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| AnalysisError(format!("read {}: {}", path.display(), e)))?;
    let raw: RawOutput = serde_json::from_str(&text)
        .map_err(|e| AnalysisError(format!("parse {}: {}", path.display(), e)))?;
    let mut segments = Vec::with_capacity(raw.segments.len());
    for s in raw.segments {
        let start_ms = (s.start * 1000.0).max(0.0) as u32;
        let end_ms = (s.end * 1000.0).max(s.start * 1000.0) as u32;
        segments.push(Segment {
            start_ms,
            end_ms,
            kind: Phrase::from_allin1_label(&s.label),
        });
    }
    Ok(AnalysisResult { segments })
}
