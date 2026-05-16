//! Shared logic for resolving which Python interpreter to use for the
//! analysis helpers (madmom beat grid, allin1 segmentation, librosa M/P,
//! essentia PitchYinFFT).
//!
//! Both the simulator's `beatgrid_gen` and LUCHS' `analysis::{allin1,
//! mp_pitch}` go through this so the user doesn't need to set
//! `LUCHS_PYTHON` manually when the standard all-in-one venv is present.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Resolve which python interpreter to use for the helpers:
/// 1. `$LUCHS_PYTHON` if set and non-empty
/// 2. `~/Python/all-in-one/.venv/bin/python` if it exists (the venv we set
///    up for LUCHS — has madmom + librosa + essentia + allin1 already)
/// 3. `python3` from PATH (likely missing the analysis deps — analysis
///    helpers will fall through to their stubs / defaults)
pub fn resolve_python() -> String {
    static CACHED: OnceLock<String> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            if let Ok(p) = std::env::var("LUCHS_PYTHON") {
                if !p.is_empty() {
                    return p;
                }
            }
            if let Ok(home) = std::env::var("HOME") {
                let candidate = PathBuf::from(&home).join("Python/all-in-one/.venv/bin/python");
                if candidate.exists() {
                    log::info!(
                        "LUCHS_PYTHON not set; auto-detected analysis python at {}",
                        candidate.display()
                    );
                    return candidate.to_string_lossy().into_owned();
                }
            }
            log::warn!(
                "LUCHS_PYTHON not set and no all-in-one venv found at $HOME/Python/all-in-one/.venv. \
                 Analysis helpers will fall back to stubs / constant BPM. Set LUCHS_PYTHON to a \
                 venv with madmom + librosa + essentia + allin1 installed."
            );
            "python3".to_string()
        })
        .clone()
}
