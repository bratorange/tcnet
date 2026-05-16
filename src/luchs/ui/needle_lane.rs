use egui::{Color32, Rect, Sense, Stroke, Ui, Vec2};

use crate::BeatGridEntry;
use crate::luchs::deck_state::{DeckRole, DeckState};
use crate::luchs::phrase_types::{magma_color, MpCurve, Segment};

use super::palette::{NEXT_BLUE, ON_AIR_RED, TEXT_DIM, TEXT_PRIMARY};
use super::pitch_contour;

const LABEL_W: f32 = 64.0;
const LANE_BG: Color32 = Color32::from_rgb(0x10, 0x10, 0x14);
const NEEDLE_POSITION: f32 = 0.22;
const VISIBLE_BARS: f32 = 8.0;

/// Color for un-phrased waveform columns (until Phase 4 wires phrase tinting).
const WAVE_FG_LOW: Color32 = Color32::from_rgb(0x46, 0x68, 0xA0);
const WAVE_FG_MID: Color32 = Color32::from_rgb(0x55, 0xAA, 0xCC);
const WAVE_FG_HIGH: Color32 = Color32::from_rgb(0xEE, 0xAA, 0x55);

pub fn show(ui: &mut Ui, deck: &DeckState, height: f32) {
    let avail_w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, height), Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, LANE_BG);
    if deck.role == DeckRole::Empty {
        // Render only background — no track loaded.
        return;
    }

    let opacity = if deck.role == DeckRole::Idle { 0.32 } else { 1.0 };
    let alpha = (opacity * 255.0) as u8;
    let dim = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha);

    // --- left label column ---
    let label_rect = Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + LABEL_W, rect.bottom()));
    let label_color = match deck.role {
        DeckRole::OnAir => ON_AIR_RED,
        DeckRole::Next => NEXT_BLUE,
        _ => TEXT_DIM,
    };
    painter.text(
        egui::pos2(label_rect.left() + 8.0, label_rect.center().y - 6.0),
        egui::Align2::LEFT_CENTER,
        format!("DECK {}", deck.layer_idx),
        egui::FontId::monospace(11.0),
        dim(label_color),
    );
    painter.text(
        egui::pos2(label_rect.left() + 8.0, label_rect.center().y + 8.0),
        egui::Align2::LEFT_CENTER,
        format!("{:.1} BPM", deck.snap.bpm.as_f32()),
        egui::FontId::monospace(9.0),
        dim(TEXT_DIM),
    );

    // --- canvas area ---
    let canvas = Rect::from_min_max(
        egui::pos2(rect.left() + LABEL_W, rect.top()),
        rect.right_bottom(),
    );
    if canvas.width() <= 4.0 {
        return;
    }

    let bpm = deck.snap.bpm.as_f32();
    if bpm <= 1.0 {
        return; // no beat info → nothing meaningful to draw yet
    }
    let beat_duration_ms = 60_000.0 / bpm;
    let bar_duration_ms = beat_duration_ms * 4.0;
    let window_duration_ms = bar_duration_ms * VISIBLE_BARS;

    let position_ms = deck.predicted_position_ms as f32;
    let needle_x = canvas.left() + NEEDLE_POSITION * canvas.width();
    let window_start_ms = position_ms - NEEDLE_POSITION * window_duration_ms;

    let ms_to_x = |ms: f32| -> f32 {
        canvas.left() + (ms - window_start_ms) / window_duration_ms * canvas.width()
    };

    // Bottom of the lane stacks (in order, from top):
    //   waveform / pitch contour / beat grid (wave_canvas)
    //   M/P gradient strip (mp_canvas, 6px)
    //   phrase strip (phrase_canvas, 14px)
    let phrase_strip_h = 14.0_f32.min(canvas.height() * 0.24);
    let mp_strip_h = 6.0_f32.min(canvas.height() * 0.12);
    let wave_canvas = Rect::from_min_max(
        canvas.left_top(),
        egui::pos2(canvas.right(), canvas.bottom() - phrase_strip_h - mp_strip_h),
    );
    let mp_canvas = Rect::from_min_max(
        egui::pos2(canvas.left(), canvas.bottom() - phrase_strip_h - mp_strip_h),
        egui::pos2(canvas.right(), canvas.bottom() - phrase_strip_h),
    );
    let phrase_canvas = Rect::from_min_max(
        egui::pos2(canvas.left(), canvas.bottom() - phrase_strip_h),
        canvas.right_bottom(),
    );

    // --- waveform ---
    if let Some(bytes) = deck.big_waveform_bytes.as_deref() {
        draw_waveform_window(
            &painter,
            wave_canvas,
            bytes,
            deck.snap.track_length_ms,
            window_start_ms,
            window_duration_ms,
            dim,
        );
    }

    // --- beat grid lines (on top of waveform so they stay legible against
    // dense bars; spec §3.7 says "behind the waveform" but in practice that
    // makes them invisible) ---
    draw_beat_grid_lines(
        &painter,
        wave_canvas,
        window_start_ms,
        window_duration_ms,
        beat_duration_ms,
        deck.beat_grid.as_deref().map(Vec::as_slice),
        deck.snap.track_length_ms,
        alpha,
    );

    // --- pitch contour (on top of grid) ---
    if let Some(contour) = deck.pitch_contour.as_deref() {
        pitch_contour::draw(
            &painter,
            wave_canvas,
            contour,
            window_start_ms,
            window_duration_ms,
            alpha,
        );
    }

    // --- M/P gradient strip (between waveform and phrase strip) ---
    draw_mp_strip(
        &painter,
        mp_canvas,
        deck.mp_curve.as_deref(),
        window_start_ms,
        window_duration_ms,
        alpha,
    );

    // --- phrase strip (bottom of lane) ---
    draw_phrase_strip(
        &painter,
        phrase_canvas,
        deck.segments.as_deref().map(Vec::as_slice),
        window_start_ms,
        window_duration_ms,
        alpha,
    );

    // --- bar numbers (top of waveform area, on top of everything) ---
    draw_bar_numbers(
        &painter,
        wave_canvas,
        window_start_ms,
        window_duration_ms,
        beat_duration_ms,
        deck.beat_grid.as_deref().map(Vec::as_slice),
        deck.snap.track_length_ms,
        alpha,
    );

    // --- needle on top ---
    let needle_color = match deck.role {
        DeckRole::OnAir => ON_AIR_RED,
        DeckRole::Next => NEXT_BLUE,
        _ => Color32::from_rgb(0xCC, 0x55, 0x55),
    };
    painter.line_segment(
        [egui::pos2(needle_x, canvas.top()), egui::pos2(needle_x, canvas.bottom())],
        Stroke::new(2.0, dim(needle_color)),
    );

    let _ = ms_to_x;
    let _ = TEXT_PRIMARY;
}

fn draw_waveform_window(
    painter: &egui::Painter,
    canvas: Rect,
    bytes: &[u8],
    track_length_ms: u32,
    window_start_ms: f32,
    window_duration_ms: f32,
    dim: impl Fn(Color32) -> Color32,
) {
    if track_length_ms == 0 || bytes.len() < 2 {
        return;
    }
    let total_cols = bytes.len() / 2;
    let canvas_h = canvas.height();
    let canvas_mid = canvas.center().y;
    let canvas_w = canvas.width();
    let ms_per_col = track_length_ms as f32 / total_cols as f32;

    let bar_w: f32 = 2.0;
    let n_bars = (canvas_w / bar_w).floor() as usize;
    if n_bars == 0 {
        return;
    }
    let ms_per_bar = window_duration_ms / n_bars as f32;
    let cols_per_bar = (ms_per_bar / ms_per_col).max(1.0);

    for i in 0..n_bars {
        let bar_start_ms = window_start_ms + i as f32 * ms_per_bar;
        let col_start_f = bar_start_ms / ms_per_col;
        let col_end_f = col_start_f + cols_per_bar;
        let col_start = col_start_f.floor() as i64;
        let col_end = col_end_f.ceil() as i64;

        let mut max_level = 0u8;
        let mut max_band = 0x04u8;
        let mut had_data = false;
        for c in col_start..col_end {
            if c < 0 || c >= total_cols as i64 {
                continue;
            }
            had_data = true;
            let lvl = bytes[c as usize * 2];
            if lvl > max_level {
                max_level = lvl;
                max_band = bytes[c as usize * 2 + 1];
            }
        }
        if !had_data {
            continue;
        }

        let level = max_level as f32 / 255.0;
        let color = band_color(max_band);
        let x = canvas.left() + i as f32 * bar_w;
        let half_h = (level * canvas_h * 0.45).max(0.5);
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x, canvas_mid - half_h),
                egui::pos2(x + bar_w, canvas_mid + half_h),
            ),
            0.0,
            dim(color),
        );
    }
}

fn band_color(band: u8) -> Color32 {
    match band {
        0x03 => WAVE_FG_LOW,
        0x04 => WAVE_FG_MID,
        0x05 => WAVE_FG_HIGH,
        _ => WAVE_FG_MID,
    }
}

fn draw_mp_strip(
    painter: &egui::Painter,
    rect: Rect,
    curve: Option<&MpCurve>,
    window_start_ms: f32,
    window_duration_ms: f32,
    alpha: u8,
) {
    // Strip background — visible even when the curve hasn't been computed
    // yet so the eye picks up the empty region.
    painter.rect_filled(
        rect,
        0.0,
        Color32::from_rgba_unmultiplied(0x12, 0x12, 0x16, alpha),
    );

    let Some(curve) = curve else { return };
    if curve.times_ms.is_empty()
        || curve.times_ms.len() != curve.melodic_ratio.len()
    {
        return;
    }

    let bar_w: f32 = 2.0;
    let n_bars = (rect.width() / bar_w).floor() as usize;
    if n_bars == 0 {
        return;
    }

    let mut cur = 0usize;
    for i in 0..n_bars {
        let t_ms = window_start_ms + (i as f32 + 0.5) / n_bars as f32 * window_duration_ms;
        if t_ms < 0.0 {
            continue;
        }
        // Walk the cursor up to t_ms (curves are sorted by time).
        while cur + 1 < curve.times_ms.len()
            && (curve.times_ms[cur + 1] as f32) < t_ms
        {
            cur += 1;
        }
        let ratio = curve.melodic_ratio[cur];
        let color = magma_color(ratio);
        let x = rect.left() + i as f32 * bar_w;
        let final_color = Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            alpha,
        );
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x, rect.top()),
                egui::pos2(x + bar_w, rect.bottom()),
            ),
            0.0,
            final_color,
        );
    }
}

fn draw_phrase_strip(
    painter: &egui::Painter,
    rect: Rect,
    segments: Option<&[Segment]>,
    window_start_ms: f32,
    window_duration_ms: f32,
    alpha: u8,
) {
    // Background for the strip even when no segments yet.
    painter.rect_filled(
        rect,
        0.0,
        Color32::from_rgba_unmultiplied(0x12, 0x12, 0x16, alpha),
    );
    let Some(segs) = segments else { return };
    let w = rect.width();
    let mid_x = |ms: f32| rect.left() + (ms - window_start_ms) / window_duration_ms * w;
    let window_end_ms = window_start_ms + window_duration_ms;
    const MIN_LABEL_W: f32 = 28.0;

    for seg in segs {
        let s_ms = seg.start_ms as f32;
        let e_ms = seg.end_ms as f32;
        if e_ms <= window_start_ms || s_ms >= window_end_ms {
            continue;
        }
        let x0 = mid_x(s_ms.max(window_start_ms));
        let x1 = mid_x(e_ms.min(window_end_ms));
        if x1 <= x0 + 0.5 {
            continue;
        }
        let bg = seg.kind.bg_color();
        let fg = seg.kind.fg_color();
        let bg_alpha = (alpha as u16 * 230 / 255) as u8;
        let bg_color = Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), bg_alpha);
        let seg_rect = Rect::from_min_max(
            egui::pos2(x0, rect.top()),
            egui::pos2((x1 - 0.5).max(x0 + 0.5), rect.bottom()),
        );
        painter.rect_filled(seg_rect, 0.0, bg_color);

        if seg_rect.width() >= MIN_LABEL_W {
            let fg_color = Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), alpha);
            painter.text(
                seg_rect.center(),
                egui::Align2::CENTER_CENTER,
                seg.kind.display_label(),
                egui::FontId::monospace(9.0),
                fg_color,
            );
        }
    }
}

fn draw_beat_grid_lines(
    painter: &egui::Painter,
    canvas: Rect,
    window_start_ms: f32,
    window_duration_ms: f32,
    beat_duration_ms: f32,
    grid: Option<&[BeatGridEntry]>,
    track_length_ms: u32,
    alpha: u8,
) {
    let canvas_w = canvas.width();
    let ms_to_x =
        |ms: f32| canvas.left() + (ms - window_start_ms) / window_duration_ms * canvas_w;

    // Two-tier grid per user request: downbeat dominant, every other beat
    // (2, 3, 4) clearly visible but lower-contrast. Original spec had three
    // tiers but beats 2 & 4 were nearly invisible against the waveform.
    let downbeat = Color32::from_rgba_unmultiplied(
        0xFF,
        0xFF,
        0xFF,
        ((alpha as u16 * 220) / 255) as u8,
    );
    let upbeat = Color32::from_rgba_unmultiplied(
        0xFF,
        0xFF,
        0xFF,
        ((alpha as u16 * 110) / 255) as u8,
    );

    let draw_for_beat = |ts: f32, beat_number_1based: u32| {
        let x = ms_to_x(ts);
        let beat_in_bar = (beat_number_1based as i64 - 1).rem_euclid(4);
        if beat_in_bar == 0 {
            // Beat 1 (downbeat) — full height, thick, brightest.
            painter.line_segment(
                [egui::pos2(x, canvas.top()), egui::pos2(x, canvas.bottom())],
                Stroke::new(1.5, downbeat),
            );
        } else {
            // Beats 2, 3, 4 — all visible, half height, centered.
            let h = canvas.height() * 0.70;
            let mid = canvas.center().y;
            painter.line_segment(
                [egui::pos2(x, mid - h / 2.0), egui::pos2(x, mid + h / 2.0)],
                Stroke::new(0.8, upbeat),
            );
        }
    };

    if let Some(entries) = grid {
        for entry in entries {
            let ts = entry.beat_timestamp as f32;
            if ts < window_start_ms - 200.0
                || ts > window_start_ms + window_duration_ms + 200.0
            {
                continue;
            }
            draw_for_beat(ts, entry.beat_number as u32);
        }
    } else if track_length_ms > 0 {
        let first_beat = ((window_start_ms / beat_duration_ms).floor() as i64).max(0);
        let last_beat = ((window_start_ms + window_duration_ms) / beat_duration_ms).ceil() as i64;
        for b in first_beat..=last_beat {
            let ts = b as f32 * beat_duration_ms;
            if ts < 0.0 || ts > track_length_ms as f32 {
                continue;
            }
            draw_for_beat(ts, (b + 1) as u32);
        }
    }
}

fn draw_bar_numbers(
    painter: &egui::Painter,
    canvas: Rect,
    window_start_ms: f32,
    window_duration_ms: f32,
    beat_duration_ms: f32,
    grid: Option<&[BeatGridEntry]>,
    track_length_ms: u32,
    alpha: u8,
) {
    let canvas_w = canvas.width();
    let ms_to_x =
        |ms: f32| canvas.left() + (ms - window_start_ms) / window_duration_ms * canvas_w;
    let label_color = Color32::from_rgba_unmultiplied(0xDD, 0xDD, 0xE6, alpha);
    let shadow_color = Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, alpha.saturating_mul(180) / 255);

    let draw_bar = |x: f32, bar_n: u32| {
        // Subtle shadow improves legibility over the waveform.
        painter.text(
            egui::pos2(x + 3.0, canvas.top() + 5.0),
            egui::Align2::LEFT_TOP,
            bar_n.to_string(),
            egui::FontId::monospace(9.0),
            shadow_color,
        );
        painter.text(
            egui::pos2(x + 2.0, canvas.top() + 4.0),
            egui::Align2::LEFT_TOP,
            bar_n.to_string(),
            egui::FontId::monospace(9.0),
            label_color,
        );
    };

    if let Some(entries) = grid {
        for entry in entries {
            if entry.beat_type != 20 {
                continue;
            }
            let ts = entry.beat_timestamp as f32;
            if ts < window_start_ms - 200.0
                || ts > window_start_ms + window_duration_ms + 200.0
            {
                continue;
            }
            let bar_n = ((entry.beat_number as u32 - 1) / 4) + 1;
            draw_bar(ms_to_x(ts), bar_n);
        }
    } else if track_length_ms > 0 {
        let first_beat = ((window_start_ms / beat_duration_ms).floor() as i64).max(0);
        let last_beat = ((window_start_ms + window_duration_ms) / beat_duration_ms).ceil() as i64;
        for b in first_beat..=last_beat {
            if b % 4 != 0 {
                continue;
            }
            let ts = b as f32 * beat_duration_ms;
            if ts < 0.0 || ts > track_length_ms as f32 {
                continue;
            }
            let bar_n = (b / 4 + 1) as u32;
            draw_bar(ms_to_x(ts), bar_n);
        }
    }
}
