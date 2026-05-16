use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::{BeatGridEntry, LayerSnapshot};

use super::phrase_types::{AnalysisState, MpCurve, Phrase, PitchContour, Segment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckRole {
    OnAir,
    Next,
    Idle,
    Empty,
}

impl DeckRole {
    pub fn opacity(self) -> f32 {
        match self {
            DeckRole::OnAir | DeckRole::Next => 1.0,
            DeckRole::Idle => super::ui::palette::IDLE_DIM_FACTOR,
            DeckRole::Empty => 0.0,
        }
    }

    pub fn short_label(self, deck_idx: u8) -> String {
        match self {
            DeckRole::OnAir => format!("DECK {} ▶ ON AIR", deck_idx),
            DeckRole::Next => format!("DECK {} ▷ NEXT", deck_idx),
            DeckRole::Idle => format!("DECK {} ‖ CUE", deck_idx),
            DeckRole::Empty => format!("DECK {}", deck_idx),
        }
    }
}

/// Per-deck state that LUCHS holds between frames.
///
/// `snap` is refreshed each frame from `DjControllerView`. `role` is computed
/// from the snapshot + mixer state by `on_air::compute_roles`. The remaining
/// fields will grow in later phases (waveforms in P3, analysis in P4, etc.).
#[derive(Clone, Debug)]
pub struct DeckState {
    pub layer_idx: u8,
    pub snap: LayerSnapshot,
    pub role: DeckRole,

    /// Smoothed playhead, updated every frame between Time-packet anchors.
    pub predicted_position_ms: u32,
    pub last_snap_position_ms: u32,
    pub last_position_update: Instant,

    /// TCNet response artifacts, filled by `WaveformPuller`.
    /// `small_waveform_bytes` is 2400 bytes — `[level, color]` pairs ×1200.
    /// `big_waveform_bytes` is the concatenated payload bytes from the first
    /// (and currently only) BigWaveform packet — same `[level, color]` layout
    /// at higher density.
    pub small_waveform_bytes: Option<Arc<Vec<u8>>>,
    pub big_waveform_bytes: Option<Arc<Vec<u8>>>,
    pub beat_grid: Option<Arc<Vec<BeatGridEntry>>>,

    /// Analysis-pipeline results (filled by `AnalysisManager` events).
    pub analysis_state: AnalysisState,
    pub segments: Option<Arc<Vec<Segment>>>,
    pub mp_curve: Option<Arc<MpCurve>>,
    pub pitch_contour: Option<Arc<PitchContour>>,

    /// Resolved audio path for the currently-loaded track (used by analysis).
    pub audio_path: Option<PathBuf>,

    /// Set to `true` once we have attempted (and failed) to resolve the audio
    /// path from the current `--media-dir` for this track. Drives the
    /// "audio missing" indicator on the deck card.
    pub audio_path_missing: bool,

    /// Last beat_number we emitted to OSC. Used to detect new beats.
    pub last_emitted_beat: u32,
    /// Last (phrase_kind, segment_index) emitted to OSC.
    /// Resets to `None` on track change so the first segment always fires.
    pub last_emitted_segment: Option<(Phrase, usize)>,
}

impl DeckState {
    pub fn new(layer_idx: u8) -> Self {
        Self {
            layer_idx,
            snap: LayerSnapshot::default(),
            role: DeckRole::Empty,
            predicted_position_ms: 0,
            last_snap_position_ms: 0,
            last_position_update: Instant::now(),
            small_waveform_bytes: None,
            big_waveform_bytes: None,
            beat_grid: None,
            analysis_state: AnalysisState::NotStarted,
            segments: None,
            mp_curve: None,
            pitch_contour: None,
            audio_path: None,
            audio_path_missing: false,
            last_emitted_beat: 0,
            last_emitted_segment: None,
        }
    }

    /// Clear waveform/beat-grid state (called on track change).
    pub fn clear_waveform(&mut self) {
        self.small_waveform_bytes = None;
        self.big_waveform_bytes = None;
        self.beat_grid = None;
    }

    /// Clear analysis state (called on track change).
    pub fn clear_analysis(&mut self) {
        self.analysis_state = AnalysisState::NotStarted;
        self.segments = None;
        self.mp_curve = None;
        self.pitch_contour = None;
        self.audio_path = None;
        self.audio_path_missing = false;
        self.last_emitted_beat = 0;
        self.last_emitted_segment = None;
    }

    /// Find the segment + index covering `predicted_position_ms`, if any.
    pub fn current_segment_with_index(&self) -> Option<(usize, &Segment)> {
        let segs = self.segments.as_deref()?;
        segs.iter()
            .enumerate()
            .find(|(_, s)| s.contains_ms(self.predicted_position_ms))
    }

    /// Return the segment covering `predicted_position_ms`, if any.
    pub fn current_segment(&self) -> Option<&Segment> {
        let segs = self.segments.as_deref()?;
        segs.iter().find(|s| s.contains_ms(self.predicted_position_ms))
    }

    /// True when no track is loaded — empty title/artist *and* no track id.
    pub fn is_empty(&self) -> bool {
        self.snap.track_id == 0
            && self.snap.title.is_empty()
            && self.snap.artist.is_empty()
            && self.snap.name.is_empty()
    }

    /// Ingest a new snapshot. Anchors the predicted playhead to the snap
    /// whenever the snap's position changes, then advances the prediction
    /// between snaps using the latest speed.
    pub fn ingest_snapshot(&mut self, snap: LayerSnapshot, now: Instant) {
        let prev_pos = self.last_snap_position_ms;
        let new_pos = snap.position_ms;
        if new_pos != prev_pos {
            self.predicted_position_ms = new_pos;
            self.last_snap_position_ms = new_pos;
            self.last_position_update = now;
        } else if snap.state.is_playing() {
            let elapsed_ms = now.duration_since(self.last_position_update).as_millis() as u32;
            let speed_ratio = snap.speed.0 as f32 / 32_768.0;
            let advance = (elapsed_ms as f32 * speed_ratio).round() as u32;
            self.predicted_position_ms = new_pos.saturating_add(advance);
        } else {
            self.predicted_position_ms = new_pos;
            self.last_position_update = now;
        }
        self.snap = snap;
    }
}
