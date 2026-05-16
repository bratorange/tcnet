use egui::Color32;
use serde::{Deserialize, Serialize};

/// Coarse phrase category used by LUCHS. We collapse the 10 allin1 labels
/// (`intro`, `outro`, `inst`, `solo`, `verse`, `chorus`, `break`, `bridge`,
/// `start`, `end`) into the 4 visual buckets the spec defines, plus an
/// `Unknown` fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phrase {
    Inst,
    Chorus,
    Verse,
    Break,
    Unknown,
}

impl Phrase {
    pub fn as_osc_int(self) -> i32 {
        match self {
            Phrase::Inst => 0,
            Phrase::Chorus => 1,
            Phrase::Verse => 2,
            Phrase::Break => 3,
            Phrase::Unknown => -1,
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Phrase::Inst => "inst",
            Phrase::Chorus => "chorus",
            Phrase::Verse => "verse",
            Phrase::Break => "break",
            Phrase::Unknown => "—",
        }
    }

    pub fn bg_color(self) -> Color32 {
        match self {
            Phrase::Inst => Color32::from_rgb(0x3A, 0x28, 0x00),
            Phrase::Chorus => Color32::from_rgb(0x00, 0x3A, 0x28),
            Phrase::Verse => Color32::from_rgb(0x00, 0x28, 0x3A),
            Phrase::Break => Color32::from_rgb(0x2A, 0x00, 0x30),
            Phrase::Unknown => Color32::from_rgb(0x18, 0x18, 0x1C),
        }
    }

    pub fn fg_color(self) -> Color32 {
        match self {
            Phrase::Inst => Color32::from_rgb(0xFF, 0xAA, 0x00),
            Phrase::Chorus => Color32::from_rgb(0x44, 0xAA, 0xFF),
            Phrase::Verse => Color32::from_rgb(0x66, 0xCC, 0xFF),
            Phrase::Break => Color32::from_rgb(0xDD, 0x66, 0xAA),
            Phrase::Unknown => Color32::from_rgb(0x60, 0x60, 0x68),
        }
    }

    /// Map the raw label emitted by `allin1` into a LUCHS phrase bucket.
    pub fn from_allin1_label(label: &str) -> Self {
        match label.to_ascii_lowercase().as_str() {
            "chorus" => Phrase::Chorus,
            "verse" => Phrase::Verse,
            "break" | "bridge" => Phrase::Break,
            "intro" | "outro" | "inst" | "solo" | "start" | "end" => Phrase::Inst,
            _ => Phrase::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub kind: Phrase,
}

impl Segment {
    pub fn duration_ms(&self) -> u32 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    pub fn contains_ms(&self, ms: u32) -> bool {
        ms >= self.start_ms && ms < self.end_ms
    }
}

/// Time-sampled melodic/percussive ratio (0.0 = fully percussive, 1.0 = fully
/// melodic). The two arrays have equal length.
#[derive(Debug, Clone, Default)]
pub struct MpCurve {
    pub times_ms: Vec<u32>,
    pub melodic_ratio: Vec<f32>,
}

/// Optional pitch contour (Phase 5 wires the renderer). `voiced[i]` is true
/// when `f0_hz[i]` is a real pitch estimate; otherwise the value should be
/// rendered as a gap.
#[derive(Debug, Clone, Default)]
pub struct PitchContour {
    pub times_ms: Vec<u32>,
    pub f0_hz: Vec<f32>,
    pub voiced: Vec<bool>,
}

/// Status of the per-deck analysis job.
#[derive(Debug, Clone, Default)]
pub enum AnalysisState {
    #[default]
    NotStarted,
    Queued,
    Running {
        /// 0.0..1.0; coarse — we get only a couple of progress events
        /// (mp/pitch done, then phrase done).
        progress: f32,
    },
    Done,
    Failed {
        reason: String,
    },
}

/// Sample the Magma colormap at `t` in [0,1]. 8-stop linear interpolation
/// matching matplotlib's `magma` colormap closely enough for a status strip.
pub fn magma_color(t: f32) -> Color32 {
    const STOPS: [(f32, [u8; 3]); 8] = [
        (0.0, [0x00, 0x00, 0x04]),
        (0.143, [0x1D, 0x11, 0x47]),
        (0.286, [0x51, 0x07, 0x6E]),
        (0.429, [0x87, 0x22, 0x6A]),
        (0.571, [0xB6, 0x3C, 0x55]),
        (0.714, [0xE0, 0x59, 0x35]),
        (0.857, [0xF8, 0x9B, 0x18]),
        (1.0, [0xFB, 0xFC, 0xBF]),
    ];
    let t = t.clamp(0.0, 1.0);
    for w in STOPS.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t <= t1 {
            let f = if t1 == t0 { 0.0 } else { (t - t0) / (t1 - t0) };
            return Color32::from_rgb(
                lerp(c0[0], c1[0], f),
                lerp(c0[1], c1[1], f),
                lerp(c0[2], c1[2], f),
            );
        }
    }
    Color32::from_rgb(STOPS[7].1[0], STOPS[7].1[1], STOPS[7].1[2])
}

fn lerp(a: u8, b: u8, f: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * f) as u8
}
