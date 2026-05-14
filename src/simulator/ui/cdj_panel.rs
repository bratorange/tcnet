use egui::{Color32, Painter, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};
use crate::active_node::ActiveDJNode;
use crate::simulator::cdj_deck::CDJDeck;

const CDJ_BG: Color32 = Color32::from_rgb(30, 30, 30);
const CDJ_ACCENT: Color32 = Color32::from_rgb(220, 120, 20);   // CDJ orange
const CDJ_TEXT: Color32 = Color32::from_rgb(220, 220, 220);
const CDJ_DIM: Color32 = Color32::from_rgb(80, 80, 80);
const CDJ_PLAYHEAD: Color32 = Color32::from_rgb(255, 80, 80);
const CDJ_BTN_ACTIVE: Color32 = Color32::from_rgb(80, 200, 80);
const CDJ_BTN: Color32 = Color32::from_rgb(55, 55, 55);

/// Returns true if the browser LOAD button was pressed, carrying the deck index.
pub struct CdjPanelResult {
    pub open_browser: bool,
}

pub fn show(
    ui: &mut Ui,
    deck: &mut CDJDeck,
    node: &mut ActiveDJNode,
    label: &str,
) -> CdjPanelResult {
    let mut result = CdjPanelResult { open_browser: false };
    let total_width = 380.0;
    let total_height = 620.0;

    let (rect, _) = ui.allocate_exact_size(vec2(total_width, total_height), Sense::hover());
    let child_ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 8.0, CDJ_BG);
    painter.rect_stroke(rect, 8.0, Stroke::new(2.0, CDJ_DIM), egui::StrokeKind::Inside);

    // Deck label
    painter.text(
        rect.center_top() + vec2(0.0, 12.0),
        egui::Align2::CENTER_TOP,
        label,
        egui::FontId::proportional(14.0),
        CDJ_ACCENT,
    );

    let inner = Rect::from_min_size(rect.min + vec2(12.0, 30.0), vec2(total_width - 24.0, total_height - 42.0));

    // --- Display area ---
    let display_rect = Rect::from_min_size(inner.min, vec2(inner.width(), 140.0));
    painter.rect_filled(display_rect, 4.0, Color32::from_rgb(10, 10, 10));

    let track_title = deck.loaded_track.as_ref().map(|t| t.title.as_str()).unwrap_or("-- NO TRACK --");
    let track_artist = deck.loaded_track.as_ref().map(|t| t.artist.as_str()).unwrap_or("");
    painter.text(
        display_rect.min + vec2(8.0, 8.0),
        egui::Align2::LEFT_TOP,
        track_title,
        egui::FontId::proportional(13.0),
        CDJ_TEXT,
    );
    painter.text(
        display_rect.min + vec2(8.0, 26.0),
        egui::Align2::LEFT_TOP,
        track_artist,
        egui::FontId::proportional(11.0),
        CDJ_DIM,
    );

    let pos_ms = deck.current_position_ms();
    let dur_ms = deck.duration_ms();
    let bpm_text = format!("BPM {:.1}", deck.bpm);
    let pos_text = CDJDeck::format_time(pos_ms);
    let dur_text = CDJDeck::format_time(dur_ms);
    let time_text = format!("{} / {}", pos_text, dur_text);

    painter.text(
        display_rect.min + vec2(8.0, 46.0),
        egui::Align2::LEFT_TOP,
        &time_text,
        egui::FontId::monospace(18.0),
        CDJ_ACCENT,
    );
    painter.text(
        display_rect.max - vec2(8.0, 14.0),
        egui::Align2::RIGHT_BOTTOM,
        &bpm_text,
        egui::FontId::monospace(13.0),
        CDJ_TEXT,
    );

    // Waveform
    let wave_rect = Rect::from_min_size(
        display_rect.min + vec2(0.0, 80.0),
        vec2(display_rect.width(), 55.0),
    );
    draw_waveform(&painter, wave_rect, deck, pos_ms, dur_ms);

    // --- Jog wheel ---
    let jog_center = inner.min + vec2(inner.width() / 2.0, 230.0);
    let jog_radius = 75.0;
    painter.circle_filled(jog_center, jog_radius, Color32::from_rgb(45, 45, 45));
    painter.circle_stroke(jog_center, jog_radius, Stroke::new(3.0, CDJ_DIM));
    painter.circle_filled(jog_center, 18.0, Color32::from_rgb(60, 60, 60));
    // Rotation indicator dot
    let angle = (pos_ms as f32 / 500.0) % std::f32::consts::TAU;
    let dot = jog_center + Vec2::angled(angle) * (jog_radius - 10.0);
    painter.circle_filled(dot, 4.0, CDJ_ACCENT);

    // --- Hot cue pads (8 pads) ---
    let pads_top = inner.min + vec2(0.0, 295.0);
    for i in 0..8 {
        let col = i % 4;
        let row = i / 4;
        let pad_size = vec2(inner.width() / 4.0 - 4.0, 28.0);
        let pad_rect = Rect::from_min_size(
            pads_top + vec2(col as f32 * (pad_size.x + 4.0), row as f32 * 32.0),
            pad_size,
        );
        painter.rect_filled(pad_rect, 3.0, Color32::from_rgb(40, 40, 80));
        painter.text(
            pad_rect.center(),
            egui::Align2::CENTER_CENTER,
            &format!("H{}", i + 1),
            egui::FontId::proportional(10.0),
            CDJ_DIM,
        );
    }

    // --- Transport buttons ---
    let btn_top = inner.min + vec2(0.0, 365.0);
    let btns: &[(&str, bool)] = &[
        ("CUE", deck.cue_ms > 0),
        ("PLAY", deck.is_playing()),
        ("SYNC", false),
    ];

    let btn_w = (inner.width() - 8.0) / 3.0;
    for (i, (label_btn, active)) in btns.iter().enumerate() {
        let btn_rect = Rect::from_min_size(
            btn_top + vec2(i as f32 * (btn_w + 4.0), 0.0),
            vec2(btn_w, 36.0),
        );
        let color = if *active { CDJ_BTN_ACTIVE } else { CDJ_BTN };
        painter.rect_filled(btn_rect, 5.0, color);
        painter.text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            label_btn,
            egui::FontId::proportional(12.0),
            CDJ_TEXT,
        );

        // Interaction
        let resp = child_ui.allocate_rect(btn_rect, Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *label_btn));
        if resp.clicked() {
            match *label_btn {
                "PLAY" => deck.toggle_play_pause(node),
                "CUE"  => deck.cue_press(node),
                _ => {}
            }
        }
    }

    // --- Tempo slider ---
    let tempo_label_pos = inner.min + vec2(0.0, 415.0);
    painter.text(
        tempo_label_pos,
        egui::Align2::LEFT_TOP,
        "TEMPO",
        egui::FontId::proportional(10.0),
        CDJ_DIM,
    );

    let slider_rect = Rect::from_min_size(
        inner.min + vec2(0.0, 428.0),
        vec2(inner.width(), 24.0),
    );
    let mut bpm = deck.bpm;
    let original_bpm = bpm;
    let resp = slider_horizontal(child_ui, &painter, slider_rect, &mut bpm, 60.0, 200.0, CDJ_ACCENT);
    resp.widget_info(|| egui::WidgetInfo::slider(true, bpm as f64, "TEMPO"));
    if resp.dragged() && (bpm - original_bpm).abs() > 0.05 {
        deck.set_tempo(bpm, node);
    }

    // --- Loop controls ---
    let loop_top = inner.min + vec2(0.0, 462.0);
    let loop_btns = ["IN", "OUT", "RELOOP", "×2"];
    let lbw = (inner.width() - 12.0) / 4.0;
    for (i, lbl) in loop_btns.iter().enumerate() {
        let r = Rect::from_min_size(
            loop_top + vec2(i as f32 * (lbw + 4.0), 0.0),
            vec2(lbw, 26.0),
        );
        painter.rect_filled(r, 4.0, CDJ_BTN);
        painter.text(r.center(), egui::Align2::CENTER_CENTER, lbl, egui::FontId::proportional(10.0), CDJ_DIM);
        let resp = child_ui.allocate_rect(r, Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *lbl));
    }

    // --- Browse / Load buttons ---
    let browse_top = inner.min + vec2(0.0, 500.0);
    let half_w = (inner.width() - 4.0) / 2.0;
    let load_rect = Rect::from_min_size(browse_top, vec2(half_w, 32.0));
    let browse_rect = Rect::from_min_size(browse_top + vec2(half_w + 4.0, 0.0), vec2(half_w, 32.0));

    painter.rect_filled(load_rect, 5.0, CDJ_BTN);
    painter.text(load_rect.center(), egui::Align2::CENTER_CENTER, "LOAD", egui::FontId::proportional(12.0), CDJ_TEXT);
    painter.rect_filled(browse_rect, 5.0, CDJ_ACCENT);
    painter.text(browse_rect.center(), egui::Align2::CENTER_CENTER, "BROWSE", egui::FontId::proportional(12.0), CDJ_TEXT);

    let browse_resp = child_ui.allocate_rect(browse_rect, Sense::click());
    browse_resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "BROWSE"));
    let load_resp = child_ui.allocate_rect(load_rect, Sense::click());
    load_resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "LOAD"));
    if browse_resp.clicked() || load_resp.clicked() {
        result.open_browser = true;
    }

    result
}

fn draw_waveform(painter: &Painter, rect: Rect, deck: &CDJDeck, pos_ms: u32, dur_ms: u32) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(5, 5, 5));
    if deck.waveform.is_empty() || dur_ms == 0 { return; }

    let n = deck.waveform.len();
    let w = rect.width();
    let h = rect.height();
    let bar_w = (w / n as f32).max(1.0);

    for (i, &amp) in deck.waveform.iter().enumerate() {
        let x = rect.min.x + i as f32 * bar_w;
        let bar_h = amp * h;
        let top = rect.min.y + (h - bar_h) / 2.0;
        let color = Color32::from_rgba_unmultiplied(0, 180, 255, 180);
        painter.rect_filled(
            Rect::from_min_size(pos2(x, top), vec2(bar_w.max(1.0), bar_h)),
            0.0,
            color,
        );
    }

    // Playhead
    if dur_ms > 0 {
        let t = pos_ms as f32 / dur_ms as f32;
        let px = rect.min.x + t * rect.width();
        painter.line_segment(
            [pos2(px, rect.min.y), pos2(px, rect.max.y)],
            Stroke::new(2.0, CDJ_PLAYHEAD),
        );
    }
}

/// Simple draggable horizontal slider. Returns the response.
fn slider_horizontal(
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    value: &mut f32,
    min: f32,
    max: f32,
    color: Color32,
) -> Response {
    painter.rect_filled(rect, 3.0, CDJ_BTN);
    let t = (*value - min) / (max - min);
    let handle_x = rect.min.x + t * rect.width();
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(handle_x - rect.min.x, rect.height())),
        3.0,
        color.linear_multiply(0.4),
    );
    painter.circle_filled(pos2(handle_x, rect.center().y), 8.0, color);

    let resp = ui.allocate_rect(rect, Sense::drag());
    if resp.dragged() {
        let delta = resp.drag_delta().x / rect.width();
        *value = (*value + delta * (max - min)).clamp(min, max);
    }
    resp
}
