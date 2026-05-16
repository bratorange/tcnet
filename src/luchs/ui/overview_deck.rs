use egui::{Color32, Sense, Ui, Vec2};

use crate::luchs::deck_state::{DeckRole, DeckState};
use crate::luchs::phrase_types::AnalysisState;

use super::palette::{IDLE_DIM_FACTOR, NEXT_BLUE, ON_AIR_RED, TEXT_DIM, TEXT_PRIMARY};
use super::{mp_strip, phrase_bar};

const BPM_AMBER: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x30);
const CARD_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1E);
const CARD_BORDER: Color32 = Color32::from_rgb(0x26, 0x26, 0x2C);
const HEADER_STRIP_H: f32 = 2.0;
const SIDE_STRIP_W: f32 = 2.0;
const CARD_PADDING: f32 = 8.0;

pub fn show(ui: &mut Ui, deck: &DeckState, size: Vec2) {
    let (card_rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(card_rect);

    if deck.role == DeckRole::Empty {
        // Empty decks render nothing — leaves a transparent gap so the grid
        // is still positioned correctly.
        painter.rect_filled(card_rect, 0.0, Color32::from_black_alpha(60));
        return;
    }

    painter.rect_filled(card_rect, 4.0, CARD_BG);

    let role_color = match deck.role {
        DeckRole::OnAir => Some(ON_AIR_RED),
        DeckRole::Next => Some(NEXT_BLUE),
        _ => None,
    };
    if let Some(c) = role_color {
        let top_strip = egui::Rect::from_min_max(
            card_rect.left_top(),
            egui::pos2(card_rect.right(), card_rect.top() + HEADER_STRIP_H),
        );
        let side_strip = egui::Rect::from_min_max(
            card_rect.left_top(),
            egui::pos2(card_rect.left() + SIDE_STRIP_W, card_rect.bottom()),
        );
        painter.rect_filled(top_strip, 0.0, c);
        painter.rect_filled(side_strip, 0.0, c);
    } else {
        painter.rect_stroke(
            card_rect,
            4.0,
            egui::Stroke::new(0.5, CARD_BORDER),
            egui::StrokeKind::Inside,
        );
    }

    let alpha = if deck.role == DeckRole::Idle {
        (IDLE_DIM_FACTOR * 255.0) as u8
    } else {
        255
    };
    let dim = |c: Color32| {
        if alpha == 255 {
            c
        } else {
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
        }
    };

    let inner = card_rect.shrink2(Vec2::new(CARD_PADDING, CARD_PADDING));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);

    // Header row: deck badge + title (left) + BPM (right)
    child.horizontal(|ui| {
        let badge_label = deck.role.short_label(deck.layer_idx);
        let badge_color = match deck.role {
            DeckRole::OnAir => ON_AIR_RED,
            DeckRole::Next => NEXT_BLUE,
            _ => Color32::from_rgb(0x40, 0x40, 0x48),
        };
        pill(ui, &badge_label, dim(badge_color));

        ui.add_space(6.0);

        // Audio-missing indicator: warns the VJ that the track is loaded
        // over TCNet but LUCHS could not find an audio file matching the
        // title in `--media-dir`, so analysis can't run.
        if deck.audio_path_missing {
            pill(ui, "⚠ AUDIO MISSING", dim(Color32::from_rgb(0xE0, 0x60, 0x40)));
            ui.add_space(6.0);
        }

        // Title (truncated by available width)
        let title = if deck.snap.title.is_empty() {
            deck.snap.name.clone()
        } else {
            deck.snap.title.clone()
        };

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // BPM right-aligned
            let bpm_val = deck.snap.bpm.as_f32();
            if bpm_val > 0.0 {
                let bpm_text = format!("{:.2}", bpm_val);
                let pct = deck.snap.speed.as_percent() - 100.0;
                let sub = format!("{:+.1}%", pct);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(sub)
                            .color(dim(TEXT_DIM))
                            .size(10.0),
                    ),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(bpm_text)
                            .color(dim(BPM_AMBER))
                            .size(14.0)
                            .monospace(),
                    ),
                );
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&title)
                            .color(dim(TEXT_PRIMARY))
                            .size(13.0)
                            .strong(),
                    )
                    .truncate(),
                );
            });
        });
    });

    // Metadata row: TIME, GRID, CUES
    child.horizontal(|ui| {
        let time = format_time(deck.predicted_position_ms);
        let total = format_time(deck.snap.track_length_ms);
        labeled_value(ui, "TIME", &format!("{} / {}", time, total), dim);
    });

    // Waveform area + M/P strip + phrase bar stacked vertically.
    let avail = child.available_size_before_wrap();
    let opacity = if deck.role == DeckRole::Idle {
        IDLE_DIM_FACTOR
    } else {
        1.0
    };
    let phrase_bar_h = 13.0;
    let mp_strip_h = 8.0;
    let progress_h = 12.0;
    let need_progress =
        !matches!(deck.analysis_state, AnalysisState::Done);
    let extra = phrase_bar_h
        + mp_strip_h
        + 2.0 * 2.0
        + if need_progress { progress_h + 2.0 } else { 0.0 };
    let waveform_h = (avail.y - extra - 6.0).max(20.0);
    if waveform_h > 8.0 {
        let (placeholder, _) =
            child.allocate_exact_size(Vec2::new(avail.x, waveform_h), Sense::hover());
        let p = child.painter_at(placeholder);
        p.rect_filled(
            placeholder,
            2.0,
            dim(Color32::from_rgb(0x16, 0x16, 0x1C)),
        );
        draw_overview_waveform(
            &p,
            placeholder,
            deck.small_waveform_bytes.as_deref(),
            deck.predicted_position_ms,
            deck.snap.track_length_ms,
            &dim,
        );
    }

    child.add_space(2.0);
    let mp_rect = mp_strip::show(
        &mut child,
        avail.x,
        mp_strip_h,
        deck.mp_curve.as_deref(),
        deck.snap.track_length_ms,
        deck.predicted_position_ms,
        opacity,
    );
    let _ = mp_rect;

    child.add_space(2.0);
    let _bar_rect = phrase_bar::show(
        &mut child,
        avail.x,
        phrase_bar_h,
        deck.segments.as_deref().map(Vec::as_slice),
        deck.snap.track_length_ms,
        opacity,
    );

    if need_progress {
        child.add_space(2.0);
        draw_progress(&mut child, avail.x, progress_h, &deck.analysis_state, opacity);
    }
}

fn draw_progress(ui: &mut Ui, width: f32, height: f32, state: &AnalysisState, opacity: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
    let bg = Color32::from_rgba_unmultiplied(0x14, 0x14, 0x18, alpha);
    painter.rect_filled(rect, 1.0, bg);

    let (label, frac, fill_color) = match state {
        AnalysisState::NotStarted => return,
        AnalysisState::Queued => (
            "ANALYSIS ⟳ queued",
            0.05,
            Color32::from_rgba_unmultiplied(0xE0, 0xA8, 0x30, alpha),
        ),
        AnalysisState::Running { progress } => (
            "ANALYSING...",
            (*progress).clamp(0.0, 1.0),
            Color32::from_rgba_unmultiplied(0x44, 0xAA, 0xFF, alpha),
        ),
        AnalysisState::Failed { reason: _ } => (
            "ANALYSIS FAILED",
            0.0,
            Color32::from_rgba_unmultiplied(0xE0, 0x40, 0x40, alpha),
        ),
        AnalysisState::Done => return,
    };

    let bar_height = 3.0;
    let bar_y = rect.bottom() - bar_height - 1.0;
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 60.0, bar_y),
        egui::pos2(rect.right() - 28.0, bar_y + bar_height),
    );
    painter.rect_filled(
        bar_rect,
        0.5,
        Color32::from_rgba_unmultiplied(0x2A, 0x2A, 0x32, alpha),
    );
    let fill_w = bar_rect.width() * frac;
    painter.rect_filled(
        egui::Rect::from_min_max(
            bar_rect.left_top(),
            egui::pos2(bar_rect.left() + fill_w, bar_rect.bottom()),
        ),
        0.5,
        fill_color,
    );
    painter.text(
        egui::pos2(rect.left() + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::monospace(9.0),
        Color32::from_rgba_unmultiplied(0xCC, 0xCC, 0xD4, alpha),
    );
    painter.text(
        egui::pos2(rect.right() - 4.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{}%", (frac * 100.0) as i32),
        egui::FontId::monospace(9.0),
        Color32::from_rgba_unmultiplied(0xCC, 0xCC, 0xD4, alpha),
    );
}

fn draw_overview_waveform(
    painter: &egui::Painter,
    rect: egui::Rect,
    bytes: Option<&Vec<u8>>,
    predicted_position_ms: u32,
    track_length_ms: u32,
    dim: &dyn Fn(Color32) -> Color32,
) {
    let Some(bytes) = bytes else { return };
    if bytes.len() < 2 {
        return;
    }
    let total_cols = bytes.len() / 2;
    let canvas_w = rect.width();
    let canvas_h = rect.height();
    let mid = rect.center().y;
    let max_half_h = canvas_h * 0.45;

    let played_frac = if track_length_ms > 0 {
        (predicted_position_ms as f32 / track_length_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let playhead_x = rect.left() + played_frac * canvas_w;

    // Bars are 2px wide so they survive antialiasing rounding even when many
    // columns map to one pixel. We max-pool over the source columns covered.
    let bar_w: f32 = 2.0;
    let n_bars = (canvas_w / bar_w).floor() as usize;
    if n_bars == 0 {
        return;
    }
    let cols_per_bar = (total_cols as f32 / n_bars as f32).max(1.0);

    for i in 0..n_bars {
        let col_start = (i as f32 * cols_per_bar) as usize;
        let col_end = ((i + 1) as f32 * cols_per_bar).ceil() as usize;
        let col_end = col_end.min(total_cols);
        if col_start >= total_cols {
            break;
        }
        let mut max_level = 0u8;
        let mut max_band = 0x04u8;
        for c in col_start..col_end {
            let lvl = bytes[c * 2];
            if lvl > max_level {
                max_level = lvl;
                max_band = bytes[c * 2 + 1];
            }
        }
        let level = max_level as f32 / 255.0;
        let color = match max_band {
            0x03 => Color32::from_rgb(0x46, 0x68, 0xA0),
            0x05 => Color32::from_rgb(0xEE, 0xAA, 0x55),
            _ => Color32::from_rgb(0x55, 0xAA, 0xCC),
        };
        let x = rect.left() + i as f32 * bar_w;
        let played = x < playhead_x;
        let alpha = if played { 230 } else { 102 };
        let final_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
        let half_h = (level * max_half_h).max(0.5);
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x, mid - half_h),
                egui::pos2(x + bar_w, mid + half_h),
            ),
            0.0,
            dim(final_color),
        );
    }

    // Red playhead tick line
    painter.line_segment(
        [
            egui::pos2(playhead_x, rect.top()),
            egui::pos2(playhead_x, rect.bottom()),
        ],
        egui::Stroke::new(1.0, dim(Color32::from_rgb(0xE0, 0x40, 0x40))),
    );
}

fn labeled_value(ui: &mut Ui, label: &str, value: &str, dim: impl Fn(Color32) -> Color32) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(label)
                .color(dim(TEXT_DIM))
                .size(9.0)
                .monospace(),
        ),
    );
    ui.add(
        egui::Label::new(
            egui::RichText::new(value)
                .color(dim(TEXT_PRIMARY))
                .size(11.0)
                .monospace(),
        ),
    );
}

fn pill(ui: &mut Ui, label: &str, color: Color32) {
    let padding_x = 7.0;
    let padding_y = 2.0;
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(10.0),
        Color32::WHITE,
    );
    let size = Vec2::new(
        galley.size().x + padding_x * 2.0,
        galley.size().y + padding_y * 2.0,
    );
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, color);
    painter.galley(
        rect.left_top() + Vec2::new(padding_x, padding_y),
        galley,
        Color32::WHITE,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
}

fn format_time(ms: u32) -> String {
    let total_s = ms / 1000;
    let m = total_s / 60;
    let s = total_s % 60;
    format!("{:02}:{:02}", m, s)
}
