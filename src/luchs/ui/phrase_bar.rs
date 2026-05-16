use egui::{Color32, Sense, Stroke, Ui, Vec2};

use crate::luchs::phrase_types::{Phrase, Segment};

use super::shimmer;

const MIN_LABEL_W: f32 = 22.0;

/// Render the phrase bar for a single track. Returns the allocated rect.
///
/// If `segments` is `None` the entire bar is drawn as a shimmer placeholder.
pub fn show(
    ui: &mut Ui,
    width: f32,
    height: f32,
    segments: Option<&[Segment]>,
    track_length_ms: u32,
    opacity: f32,
) -> egui::Rect {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());

    if segments.is_none() || track_length_ms == 0 {
        shimmer::draw(ui, rect);
        return rect;
    }

    let painter = ui.painter_at(rect);
    let segs = segments.unwrap();
    let total_ms = track_length_ms as f32;
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
    let with_alpha = |c: Color32| -> Color32 {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
    };

    for seg in segs {
        let start_frac = (seg.start_ms as f32 / total_ms).clamp(0.0, 1.0);
        let end_frac = (seg.end_ms as f32 / total_ms).clamp(0.0, 1.0);
        if end_frac <= start_frac {
            continue;
        }
        let x0 = rect.left() + start_frac * rect.width();
        let x1 = rect.left() + end_frac * rect.width();
        let seg_rect = egui::Rect::from_min_max(
            egui::pos2(x0, rect.top()),
            egui::pos2((x1 - 1.0).max(x0 + 1.0), rect.bottom()),
        );

        painter.rect_filled(seg_rect, 1.5, with_alpha(seg.kind.bg_color()));

        if seg_rect.width() >= MIN_LABEL_W {
            painter.text(
                seg_rect.center(),
                egui::Align2::CENTER_CENTER,
                seg.kind.display_label(),
                egui::FontId::monospace(9.0),
                with_alpha(seg.kind.fg_color()),
            );
        }
    }

    // Faint outer border.
    painter.rect_stroke(
        rect,
        1.5,
        Stroke::new(0.5, Color32::from_rgba_unmultiplied(0x40, 0x40, 0x48, alpha)),
        egui::StrokeKind::Inside,
    );

    rect
}
