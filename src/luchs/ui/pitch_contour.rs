use egui::{Color32, Rect, Stroke};

use crate::luchs::phrase_types::PitchContour;

/// Bright salmon-pink — matches the reference visualization aesthetic.
const LINE_COLOR: Color32 = Color32::from_rgb(0xFF, 0x8C, 0x82);

/// F0 (Hz) → vertical fraction in [0,1] using a log scale anchored to a
/// musically useful range (60-800 Hz covers bass through soprano). Outside
/// the range is clamped to the edges.
fn f0_to_frac(f0_hz: f32) -> f32 {
    const F_LO: f32 = 60.0;
    const F_HI: f32 = 800.0;
    let clamped = f0_hz.clamp(F_LO, F_HI);
    let f = (clamped.ln() - F_LO.ln()) / (F_HI.ln() - F_LO.ln());
    f.clamp(0.0, 1.0)
}

/// Bridge unvoiced gaps shorter than this many milliseconds so the line stays
/// continuous through brief breath/silence dropouts (PitchMelodia tends to
/// flicker at ~3ms hop intervals).
const GAP_BRIDGE_MS: u32 = 60;

/// Draw the pitch contour inside `canvas`. Renders a thin filled area below
/// the curve for depth + a salmon polyline. Skips long unvoiced gaps.
pub fn draw(
    painter: &egui::Painter,
    canvas: Rect,
    contour: &PitchContour,
    window_start_ms: f32,
    window_duration_ms: f32,
    alpha: u8,
) {
    if contour.times_ms.is_empty()
        || contour.times_ms.len() != contour.f0_hz.len()
        || contour.times_ms.len() != contour.voiced.len()
    {
        return;
    }

    let canvas_left = canvas.left();
    let canvas_w = canvas.width();
    // The pitch line occupies the central ~62% of the lane height; centred
    // on the lane so highs visually correspond to "up" and lows to "down".
    let zone_h = canvas.height() * 0.62;
    let mid_y = canvas.center().y;
    let y_top = mid_y - zone_h * 0.5;
    let y_bot = mid_y + zone_h * 0.5;

    // Compute window range as indices via binary search.
    let start_idx = contour
        .times_ms
        .partition_point(|&t| (t as f32) < window_start_ms);
    let end_idx = contour
        .times_ms
        .partition_point(|&t| (t as f32) < window_start_ms + window_duration_ms);
    if end_idx <= start_idx {
        return;
    }

    let line_color = with_alpha(LINE_COLOR, alpha);

    // Render only the polyline. The faint fill was previously a
    // `convex_polygon` of the curve + two anchor points at `y_bot`; pitch
    // contours are concave, so convex tessellation produced visible rays.
    // Skipping the fill keeps the contour clean.
    let flush = |run: &mut Vec<egui::Pos2>, painter: &egui::Painter| {
        if run.len() >= 2 {
            painter.add(egui::Shape::line(run.clone(), Stroke::new(1.8, line_color)));
        }
        run.clear();
    };
    let _ = y_bot;

    let mut current_run: Vec<egui::Pos2> = Vec::new();
    let mut last_voiced_ms: Option<u32> = None;

    for i in start_idx..end_idx {
        let voiced = contour.voiced[i];
        let t_ms = contour.times_ms[i];
        let f0 = contour.f0_hz[i];
        if !voiced || f0 <= 0.0 {
            // Don't flush yet — wait to see if the gap is short enough to bridge.
            continue;
        }
        // Bridge short unvoiced gaps.
        if let Some(prev) = last_voiced_ms {
            if t_ms.saturating_sub(prev) > GAP_BRIDGE_MS {
                flush(&mut current_run, painter);
            }
        }
        last_voiced_ms = Some(t_ms);
        let frac = f0_to_frac(f0);
        let x = canvas_left + ((t_ms as f32) - window_start_ms) / window_duration_ms * canvas_w;
        // Higher f0 → smaller y (egui y grows down).
        let y = y_bot - frac * (y_bot - y_top);
        current_run.push(egui::pos2(x, y));
    }
    flush(&mut current_run, painter);
}

fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}
