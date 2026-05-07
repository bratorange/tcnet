use egui::{Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use crate::node::dj_controller::LayerSnapshot;
use crate::SmallWaveformData;

const BADGE_W: f32 = 28.0;
const MINI_WAVE_W: f32 = 200.0;
const PADDING: f32 = 6.0;

pub fn show(
    ui: &mut egui::Ui,
    deck_num: u8,
    layer: &LayerSnapshot,
    small_wave: Option<&SmallWaveformData>,
    width: f32,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 0.0, Color32::from_rgb(18, 18, 18));
    // Bottom border
    painter.line_segment(
        [pos2(rect.min.x, rect.max.y - 1.0), pos2(rect.max.x, rect.max.y - 1.0)],
        Stroke::new(1.0, Color32::from_rgb(40, 40, 40)),
    );

    // Deck number badge
    let badge_rect = Rect::from_min_size(
        pos2(rect.min.x + PADDING, rect.min.y + PADDING),
        vec2(BADGE_W, height - PADDING * 2.0),
    );
    let badge_color = deck_badge_color(deck_num);
    painter.rect_filled(badge_rect, 3.0, badge_color);
    painter.text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        deck_num.to_string(),
        FontId::proportional(14.0),
        Color32::WHITE,
    );

    // Mini waveform area (right side)
    let wave_rect = Rect::from_min_size(
        pos2(rect.max.x - MINI_WAVE_W - PADDING, rect.min.y + PADDING),
        vec2(MINI_WAVE_W, height - PADDING * 2.0),
    );
    draw_mini_waveform(&painter, wave_rect, small_wave, layer.position_ms, layer.track_length_ms);

    // Text area between badge and mini wave
    let text_x = rect.min.x + BADGE_W + PADDING * 2.0;
    let text_right = wave_rect.min.x - PADDING;
    let text_w = (text_right - text_x).max(0.0);

    // Title
    let title = if layer.title.is_empty() { "—" } else { &layer.title };
    painter.text(
        pos2(text_x, rect.min.y + PADDING),
        egui::Align2::LEFT_TOP,
        truncate_str(title, text_w, 14.0),
        FontId::proportional(14.0),
        Color32::WHITE,
    );

    // Artist
    let artist = if layer.artist.is_empty() { "" } else { &layer.artist };
    painter.text(
        pos2(text_x, rect.min.y + PADDING + 18.0),
        egui::Align2::LEFT_TOP,
        truncate_str(artist, text_w, 12.0),
        FontId::proportional(12.0),
        Color32::from_rgb(160, 160, 160),
    );

    // Time display
    let elapsed = format_time_cs(layer.position_ms);
    painter.text(
        pos2(text_x, rect.min.y + PADDING + 38.0),
        egui::Align2::LEFT_TOP,
        elapsed,
        FontId::monospace(13.0),
        Color32::from_rgb(220, 220, 220),
    );

    // BPM box
    if layer.bpm.as_f32() > 0.0 {
        let bpm_str = format!("{:.2} BPM", layer.bpm.as_f32());
        let bpm_rect = Rect::from_min_size(
            pos2(text_x, rect.min.y + PADDING + 58.0),
            vec2(90.0, 18.0),
        );
        painter.rect_filled(bpm_rect, 2.0, Color32::from_rgb(30, 30, 30));
        painter.text(
            bpm_rect.center(),
            egui::Align2::CENTER_CENTER,
            bpm_str,
            FontId::monospace(11.0),
            Color32::from_rgb(200, 200, 200),
        );
    }

    // Play/pause indicator
    let play_x = text_x + 100.0;
    let play_y = rect.min.y + PADDING + 58.0;
    if layer.state.is_playing() {
        painter.text(
            pos2(play_x, play_y),
            egui::Align2::LEFT_TOP,
            "▶",
            FontId::proportional(16.0),
            Color32::from_rgb(60, 200, 60),
        );
    } else {
        painter.text(
            pos2(play_x, play_y),
            egui::Align2::LEFT_TOP,
            "⏸",
            FontId::proportional(16.0),
            Color32::from_rgb(120, 120, 120),
        );
    }
}

fn draw_mini_waveform(
    painter: &egui::Painter,
    rect: Rect,
    data: Option<&SmallWaveformData>,
    position_ms: u32,
    track_length_ms: u32,
) {
    painter.rect_filled(rect, 2.0, Color32::from_rgb(8, 8, 8));

    let Some(data) = data else { return; };
    let bytes = data.bytes();
    let n_samples = bytes.len() / 2; // 1200
    if n_samples == 0 { return; }

    let bar_w = rect.width() / n_samples as f32;
    let mid_y = rect.center().y;
    let half_h = rect.height() / 2.0;

    for i in 0..n_samples {
        let blevel = bytes[i * 2] as f32 / 255.0;
        let bcolor = bytes[i * 2 + 1];
        let color = bcolor_to_color32(bcolor);
        if blevel < 0.01 { continue; }
        let bar_h = blevel * half_h;
        let x = rect.min.x + i as f32 * bar_w;
        // Draw symmetrically around center
        painter.rect_filled(
            Rect::from_min_size(
                pos2(x, mid_y - bar_h),
                vec2(bar_w.max(1.0), bar_h * 2.0),
            ),
            0.0,
            color,
        );
    }

    // Playhead line
    if track_length_ms > 0 {
        let t = position_ms as f32 / track_length_ms as f32;
        let px = rect.min.x + t * rect.width();
        painter.line_segment(
            [pos2(px, rect.min.y), pos2(px, rect.max.y)],
            Stroke::new(2.0, Color32::WHITE),
        );
    }
}

fn format_time_cs(ms: u32) -> String {
    let cs = (ms / 10) % 100;
    let s = (ms / 1000) % 60;
    let m = ms / 60_000;
    format!("{:02}:{:02}.{:02}", m, s, cs)
}

fn bcolor_to_color32(bcolor: u8) -> Color32 {
    match bcolor {
        0 => Color32::TRANSPARENT,
        1..=40 => Color32::from_rgb(0, 80, 200),
        41..=80 => Color32::from_rgb(0, 200, 220),
        81..=120 => Color32::from_rgb(0, 200, 80),
        121..=160 => Color32::from_rgb(220, 200, 0),
        161..=200 => Color32::from_rgb(220, 120, 0),
        _ => Color32::from_rgb(220, 30, 30),
    }
}

fn deck_badge_color(deck_num: u8) -> Color32 {
    match deck_num {
        1 => Color32::from_rgb(160, 20, 20),
        2 => Color32::from_rgb(20, 60, 160),
        3 => Color32::from_rgb(160, 20, 20),
        4 => Color32::from_rgb(20, 60, 160),
        _ => Color32::from_rgb(60, 60, 60),
    }
}

fn truncate_str(s: &str, max_width_px: f32, font_size: f32) -> String {
    // Approximate: ~0.6 * font_size per character for proportional fonts
    let approx_char_w = font_size * 0.6;
    let max_chars = (max_width_px / approx_char_w) as usize;
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}
