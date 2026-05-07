use egui::{Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use crate::node::dj_controller::LayerSnapshot;
use crate::SmallWaveformData;

const PLAYHEAD_FRAC: f32 = 0.30;
const VISIBLE_SAMPLES: i32 = 200;
const LEFT_LABEL_W: f32 = 70.0;

pub fn show(
    ui: &mut egui::Ui,
    deck_num: usize,
    layer: &LayerSnapshot,
    small_wave: Option<&SmallWaveformData>,
    width: f32,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 0.0, Color32::BLACK);

    // Left label panel
    let label_rect = Rect::from_min_size(rect.min, vec2(LEFT_LABEL_W, height));
    painter.rect_filled(label_rect, 0.0, Color32::from_rgb(12, 12, 12));

    // Deck label
    painter.text(
        pos2(label_rect.center().x, label_rect.min.y + 14.0),
        egui::Align2::CENTER_TOP,
        format!("DECK {}", deck_num),
        FontId::monospace(9.0),
        Color32::from_rgb(140, 140, 140),
    );

    // Play/pause symbol
    let play_sym = if layer.state.is_playing() { "▶" } else { "⏸" };
    let play_color = if layer.state.is_playing() {
        Color32::from_rgb(60, 200, 60)
    } else {
        Color32::from_rgb(80, 80, 80)
    };
    painter.text(
        pos2(label_rect.center().x, label_rect.min.y + 30.0),
        egui::Align2::CENTER_TOP,
        play_sym,
        FontId::proportional(14.0),
        play_color,
    );

    // Tempo offset
    let tempo_offset = layer.speed.as_percent() - 100.0;
    let tempo_str = format!("{:+.2}%", tempo_offset);
    painter.text(
        pos2(label_rect.center().x, label_rect.min.y + 52.0),
        egui::Align2::CENTER_TOP,
        tempo_str,
        FontId::monospace(9.0),
        Color32::from_rgb(160, 160, 160),
    );
    painter.text(
        pos2(label_rect.center().x, label_rect.min.y + 64.0),
        egui::Align2::CENTER_TOP,
        "TEMPO",
        FontId::monospace(8.0),
        Color32::from_rgb(100, 100, 100),
    );

    // Vertical separator
    painter.line_segment(
        [pos2(label_rect.max.x, rect.min.y), pos2(label_rect.max.x, rect.max.y)],
        Stroke::new(1.0, Color32::from_rgb(30, 30, 30)),
    );

    // Waveform area
    let wave_rect = Rect::from_min_size(
        pos2(rect.min.x + LEFT_LABEL_W, rect.min.y),
        vec2(width - LEFT_LABEL_W, height),
    );

    draw_waveform_lane(&painter, wave_rect, small_wave, layer);

    // Bottom border
    painter.line_segment(
        [pos2(rect.min.x, rect.max.y - 1.0), pos2(rect.max.x, rect.max.y - 1.0)],
        Stroke::new(1.0, Color32::from_rgb(25, 25, 25)),
    );
}

fn draw_waveform_lane(
    painter: &egui::Painter,
    rect: Rect,
    small_wave: Option<&SmallWaveformData>,
    layer: &LayerSnapshot,
) {
    let mid_y = rect.min.y + rect.height() / 2.0;
    let half_h = rect.height() / 2.0 - 4.0;

    // Draw center line
    painter.line_segment(
        [pos2(rect.min.x, mid_y), pos2(rect.max.x, mid_y)],
        Stroke::new(1.0, Color32::from_rgb(25, 25, 25)),
    );

    // Playhead X position
    let playhead_x = rect.min.x + rect.width() * PLAYHEAD_FRAC;

    let bar_w = rect.width() / VISIBLE_SAMPLES as f32;

    if let Some(data) = small_wave {
        let bytes = data.bytes();
        let n_samples = (bytes.len() / 2) as i32; // 1200

        let center_sample = if layer.track_length_ms > 0 {
            (layer.position_ms as f32 / layer.track_length_ms as f32 * n_samples as f32) as i32
        } else {
            0
        };
        let left_sample = center_sample - (VISIBLE_SAMPLES as f32 * PLAYHEAD_FRAC) as i32;

        for col in 0..VISIBLE_SAMPLES {
            let sample_idx = left_sample + col;
            if sample_idx < 0 || sample_idx >= n_samples { continue; }

            let si = sample_idx as usize;
            let blevel = bytes[si * 2] as f32 / 255.0;
            let bcolor = bytes[si * 2 + 1];
            let color = bcolor_to_color32(bcolor);
            if blevel < 0.01 { continue; }

            let bar_h = blevel * half_h;
            let x = rect.min.x + col as f32 * bar_w;
            painter.rect_filled(
                egui::Rect::from_min_size(
                    pos2(x, mid_y - bar_h),
                    vec2(bar_w.max(1.0), bar_h * 2.0),
                ),
                0.0,
                color,
            );
        }

        // Beat grid ticks (synthetic, from BPM)
        draw_beat_grid(painter, rect, layer, left_sample, n_samples, bar_w);
    } else {
        // No waveform: show loading hint
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Loading waveform…",
            FontId::proportional(11.0),
            Color32::from_rgb(50, 50, 50),
        );
    }

    // Playhead line (drawn on top of waveform)
    painter.line_segment(
        [pos2(playhead_x, rect.min.y), pos2(playhead_x, rect.max.y)],
        Stroke::new(2.0, Color32::WHITE),
    );
}

fn draw_beat_grid(
    painter: &egui::Painter,
    rect: Rect,
    layer: &LayerSnapshot,
    left_sample: i32,
    n_samples: i32,
    bar_w: f32,
) {
    let bpm = layer.bpm.as_f32();
    if bpm < 20.0 || layer.track_length_ms == 0 { return; }

    let beat_interval_ms = 60_000.0 / bpm;
    let ms_per_sample = layer.track_length_ms as f32 / n_samples as f32;

    // How many samples per beat
    let samples_per_beat = beat_interval_ms / ms_per_sample;
    if samples_per_beat < 1.0 { return; }

    // Find first beat at or after left_sample
    let first_beat_idx = if left_sample >= 0 {
        (left_sample as f32 / samples_per_beat).ceil() as i32
    } else {
        0
    };

    let mut beat_sample = first_beat_idx as f32 * samples_per_beat;
    let right_sample = left_sample + VISIBLE_SAMPLES;

    let mut beat_counter = first_beat_idx;
    while beat_sample < right_sample as f32 {
        let col = beat_sample - left_sample as f32;
        let x = rect.min.x + col * bar_w;
        if x >= rect.min.x && x <= rect.max.x {
            let is_downbeat = beat_counter % 4 == 0;
            let tick_h = if is_downbeat { 14.0 } else { 8.0 };
            let tick_color = if is_downbeat {
                Color32::from_rgb(255, 255, 255)
            } else {
                Color32::from_rgb(140, 140, 140)
            };
            painter.line_segment(
                [pos2(x, rect.min.y), pos2(x, rect.min.y + tick_h)],
                Stroke::new(1.0, tick_color),
            );
        }
        beat_sample += samples_per_beat;
        beat_counter += 1;
    }
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
