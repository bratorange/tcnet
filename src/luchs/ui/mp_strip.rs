use egui::{Color32, Sense, Ui, Vec2};

use crate::luchs::phrase_types::{magma_color, MpCurve};

/// Draw the melodic/percussive gradient strip. Returns the allocated rect so
/// the caller can place a playhead tick on top.
///
/// `played_ms` controls which portion renders at full opacity vs. dimmed.
pub fn show(
    ui: &mut Ui,
    width: f32,
    height: f32,
    curve: Option<&MpCurve>,
    track_length_ms: u32,
    played_ms: u32,
    opacity: f32,
) -> egui::Rect {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter_at(rect);

    let base_alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
    let bg = Color32::from_rgba_unmultiplied(0x14, 0x14, 0x18, base_alpha);
    painter.rect_filled(rect, 1.0, bg);

    let Some(curve) = curve else { return rect };
    if curve.times_ms.is_empty() || track_length_ms == 0 {
        return rect;
    }

    let played_frac = (played_ms as f32 / track_length_ms as f32).clamp(0.0, 1.0);
    let playhead_x = rect.left() + played_frac * rect.width();

    let bar_w: f32 = 2.0;
    let n_bars = (rect.width() / bar_w).floor() as usize;
    if n_bars == 0 {
        return rect;
    }

    // Walk both arrays with a moving pointer so we don't rebinary-search per
    // column. `cur` advances monotonically.
    let mut cur = 0usize;
    let total_ms = track_length_ms as f32;

    for i in 0..n_bars {
        let t_ms = (i as f32 + 0.5) / n_bars as f32 * total_ms;
        while cur + 1 < curve.times_ms.len()
            && (curve.times_ms[cur + 1] as f32) < t_ms
        {
            cur += 1;
        }
        let ratio = curve.melodic_ratio[cur];
        let color = magma_color(ratio);
        let x = rect.left() + i as f32 * bar_w;
        let played = x < playhead_x;
        let alpha = if played {
            ((base_alpha as f32) * 0.9) as u8
        } else {
            ((base_alpha as f32) * 0.4) as u8
        };
        let final_color = Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            alpha,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x, rect.top()),
                egui::pos2(x + bar_w, rect.bottom()),
            ),
            0.0,
            final_color,
        );
    }

    // Red playhead tick across the strip, matching the waveform's tick.
    painter.line_segment(
        [
            egui::pos2(playhead_x, rect.top()),
            egui::pos2(playhead_x, rect.bottom()),
        ],
        egui::Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(0xE0, 0x40, 0x40, base_alpha),
        ),
    );

    rect
}
