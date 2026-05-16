use egui::{Color32, Sense, Ui, Vec2};

use crate::luchs::deck_state::{DeckRole, DeckState};
use crate::luchs::phrase_types::{Phrase, Segment};
use crate::luchs::state::LuchsState;

use super::palette::{TEXT_DIM, TEXT_PRIMARY, TOP_BAR_BG, TOP_BAR_BORDER};

const BAR_H: f32 = 24.0;
const BPM_AMBER: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x30);

pub fn show(ui: &mut Ui, state: &LuchsState) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), BAR_H), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, TOP_BAR_BG);
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.top() + 0.5),
            egui::pos2(rect.right(), rect.top() + 0.5),
        ],
        egui::Stroke::new(0.5, TOP_BAR_BORDER),
    );

    let mut x = rect.left() + 10.0;
    let y = rect.center().y;
    let font = egui::FontId::monospace(11.0);

    // MASTER — master BPM (on-air deck preferred)
    let master_deck = state
        .decks
        .iter()
        .find(|d| d.role == DeckRole::OnAir)
        .or_else(|| state.decks.iter().find(|d| d.role != DeckRole::Empty));

    let master_bpm = master_deck.map(|d| d.snap.bpm.as_f32()).unwrap_or(0.0);
    x = label(&painter, x, y, "MASTER", TEXT_DIM, &font);
    x = value(
        &painter,
        x,
        y,
        &format!("{:.2} BPM", master_bpm),
        BPM_AMBER,
        &font,
    );
    x += 14.0;

    // BAR — current bar / total bars (only meaningful for the master deck)
    if let Some(deck) = master_deck {
        let bpm = deck.snap.bpm.as_f32();
        if bpm > 1.0 {
            let beat_ms = 60_000.0 / bpm;
            let cur_bar = ((deck.predicted_position_ms as f32) / (beat_ms * 4.0))
                .floor() as i64
                + 1;
            let total_bar = ((deck.snap.track_length_ms as f32) / (beat_ms * 4.0))
                .ceil() as i64;
            x = label(&painter, x, y, "BAR", TEXT_DIM, &font);
            x = value(
                &painter,
                x,
                y,
                &format!("{} / {}", cur_bar.max(1), total_bar.max(1)),
                TEXT_PRIMARY,
                &font,
            );
            x += 14.0;
        }
    }

    // PHRASE — current phrase + bars remaining
    if let Some(deck) = master_deck {
        let phrase_info = current_phrase_info(deck);
        x = label(&painter, x, y, "PHRASE", TEXT_DIM, &font);
        match phrase_info {
            Some((phrase, bars_left, deck_idx)) => {
                x = value(
                    &painter,
                    x,
                    y,
                    phrase.display_label(),
                    phrase.fg_color(),
                    &font,
                );
                x += 6.0;
                x = value(
                    &painter,
                    x,
                    y,
                    &format!("(CDJ{}) · {} bars left", deck_idx, bars_left),
                    TEXT_PRIMARY,
                    &font,
                );
            }
            None => {
                x = value(&painter, x, y, "—", TEXT_DIM, &font);
            }
        }
        x += 14.0;
    }

    // TRANSITION — bars until next phrase boundary
    if let Some(deck) = master_deck {
        if let Some(bars_to_next) = bars_to_next_boundary(deck) {
            x = label(&painter, x, y, "TRANSITION", TEXT_DIM, &font);
            value(
                &painter,
                x,
                y,
                &format!("▲ in ~{} bars", bars_to_next),
                TEXT_PRIMARY,
                &font,
            );
        }
    }
}

fn current_phrase_info(deck: &DeckState) -> Option<(Phrase, u32, u8)> {
    let seg = deck.current_segment()?;
    let bpm = deck.snap.bpm.as_f32();
    if bpm <= 1.0 {
        return None;
    }
    let bar_ms = 60_000.0 / bpm * 4.0;
    let remaining_ms = seg.end_ms.saturating_sub(deck.predicted_position_ms) as f32;
    let bars_left = (remaining_ms / bar_ms).round() as u32;
    Some((seg.kind, bars_left, deck.layer_idx))
}

fn bars_to_next_boundary(deck: &DeckState) -> Option<u32> {
    let segs = deck.segments.as_deref()?;
    let bpm = deck.snap.bpm.as_f32();
    if bpm <= 1.0 {
        return None;
    }
    let bar_ms = 60_000.0 / bpm * 4.0;
    let next: Option<&Segment> = segs
        .iter()
        .find(|s| s.start_ms > deck.predicted_position_ms);
    let next = next?;
    let delta = (next.start_ms - deck.predicted_position_ms) as f32;
    Some((delta / bar_ms).round() as u32)
}

fn label(
    painter: &egui::Painter,
    x: f32,
    y: f32,
    text: &str,
    color: Color32,
    font: &egui::FontId,
) -> f32 {
    painter.text(
        egui::pos2(x, y),
        egui::Align2::LEFT_CENTER,
        text,
        font.clone(),
        color,
    );
    let galley_w = painter.layout_no_wrap(text.to_string(), font.clone(), color).size().x;
    x + galley_w + 6.0
}

fn value(
    painter: &egui::Painter,
    x: f32,
    y: f32,
    text: &str,
    color: Color32,
    font: &egui::FontId,
) -> f32 {
    painter.text(
        egui::pos2(x, y),
        egui::Align2::LEFT_CENTER,
        text,
        font.clone(),
        color,
    );
    let galley_w = painter.layout_no_wrap(text.to_string(), font.clone(), color).size().x;
    x + galley_w
}
