use egui::{Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use crate::active_node::ActiveDJNode;
use crate::node::dj_controller::MixerSnapshot;

const MX_BG: Color32 = Color32::from_rgb(22, 22, 22);
const MX_ACCENT: Color32 = Color32::from_rgb(60, 140, 210);
const MX_TEXT: Color32 = Color32::from_rgb(200, 200, 200);
const MX_DIM: Color32 = Color32::from_rgb(70, 70, 70);
const MX_BTN: Color32 = Color32::from_rgb(45, 45, 45);
const MX_CUE_ON: Color32 = Color32::from_rgb(255, 200, 0);

pub fn show(ui: &mut Ui, mixer: &mut MixerSnapshot, node: &mut ActiveDJNode) {
    let total_width = 340.0;
    let total_height = 620.0;

    let (rect, _) = ui.allocate_exact_size(vec2(total_width, total_height), Sense::hover());
    let child_ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 8.0, MX_BG);
    painter.rect_stroke(rect, 8.0, Stroke::new(2.0, MX_DIM), egui::StrokeKind::Inside);

    painter.text(
        rect.center_top() + vec2(0.0, 12.0),
        egui::Align2::CENTER_TOP,
        "DJM-A9",
        egui::FontId::proportional(14.0),
        MX_ACCENT,
    );

    let inner = Rect::from_min_size(rect.min + vec2(8.0, 30.0), vec2(total_width - 16.0, total_height - 42.0));

    // --- 4 Channel strips ---
    let ch_w = (inner.width() - 12.0) / 4.0;
    for ch in 0..4usize {
        let ch_rect = Rect::from_min_size(
            inner.min + vec2(ch as f32 * (ch_w + 4.0), 0.0),
            vec2(ch_w, 460.0),
        );
        draw_channel(child_ui, &painter, ch_rect, ch, mixer, node);
    }

    // --- Crossfader ---
    let xfader_top = inner.min + vec2(0.0, 470.0);
    painter.text(
        xfader_top,
        egui::Align2::LEFT_TOP,
        "CROSSFADER",
        egui::FontId::proportional(9.0),
        MX_DIM,
    );
    let xfader_rect = Rect::from_min_size(xfader_top + vec2(0.0, 12.0), vec2(inner.width(), 22.0));
    let mut xfader = mixer.crossfader as f32 / 255.0;
    let xfader_resp = horizontal_slider(child_ui, &painter, xfader_rect, &mut xfader, MX_ACCENT);
    xfader_resp.widget_info(|| egui::WidgetInfo::slider(true, xfader as f64, "CROSSFADER"));
    if xfader_resp.dragged() {
        mixer.crossfader = (xfader * 255.0) as u8;
        let _ = node.set_crossfader(mixer.crossfader);
    }

    // --- Master fader ---
    let master_top = inner.min + vec2(0.0, 510.0);
    painter.text(master_top, egui::Align2::LEFT_TOP, "MASTER", egui::FontId::proportional(9.0), MX_DIM);
    let master_rect = Rect::from_min_size(master_top + vec2(0.0, 12.0), vec2(inner.width() / 2.0 - 4.0, 22.0));
    let mut master = mixer.master_fader_level as f32 / 255.0;
    let master_resp = horizontal_slider(child_ui, &painter, master_rect, &mut master, MX_ACCENT);
    master_resp.widget_info(|| egui::WidgetInfo::slider(true, master as f64, "MASTER"));
    if master_resp.dragged() {
        mixer.master_fader_level = (master * 255.0) as u8;
        let _ = node.set_master_fader(mixer.master_fader_level);
    }

    // Booth fader
    let booth_rect = Rect::from_min_size(master_top + vec2(inner.width() / 2.0 + 4.0, 12.0), vec2(inner.width() / 2.0 - 4.0, 22.0));
    painter.text(
        master_top + vec2(inner.width() / 2.0 + 4.0, 0.0),
        egui::Align2::LEFT_TOP,
        "BOOTH",
        egui::FontId::proportional(9.0),
        MX_DIM,
    );
    let mut booth = mixer.booth_level as f32 / 255.0;
    let booth_resp = horizontal_slider(child_ui, &painter, booth_rect, &mut booth, MX_ACCENT);
    booth_resp.widget_info(|| egui::WidgetInfo::slider(true, booth as f64, "BOOTH"));
    if booth_resp.dragged() {
        mixer.booth_level = (booth * 255.0) as u8;
    }

    // Beat FX label
    let bfx_top = inner.min + vec2(0.0, 547.0);
    painter.text(bfx_top, egui::Align2::LEFT_TOP, "BEAT FX", egui::FontId::proportional(9.0), MX_DIM);
    let bfx_btn = Rect::from_min_size(bfx_top + vec2(55.0, -2.0), vec2(40.0, 16.0));
    let bfx_color = if mixer.beat_fx_on { MX_CUE_ON } else { MX_BTN };
    painter.rect_filled(bfx_btn, 3.0, bfx_color);
    painter.text(bfx_btn.center(), egui::Align2::CENTER_CENTER, "ON", egui::FontId::proportional(9.0), MX_TEXT);
    let bfx_resp = child_ui.allocate_rect(bfx_btn, Sense::click());
    bfx_resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, mixer.beat_fx_on, "BEAT FX ON"));
    if bfx_resp.clicked() {
        mixer.beat_fx_on = !mixer.beat_fx_on;
    }
}

fn draw_channel(
    ui: &mut Ui,
    painter: &egui::Painter,
    rect: Rect,
    ch: usize,
    mixer: &mut MixerSnapshot,
    node: &mut ActiveDJNode,
) {
    painter.rect_filled(rect, 4.0, Color32::from_rgb(28, 28, 28));
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, MX_DIM), egui::StrokeKind::Inside);

    // Channel label
    painter.text(
        rect.center_top() + vec2(0.0, 6.0),
        egui::Align2::CENTER_TOP,
        &format!("CH{}", ch + 1),
        egui::FontId::proportional(10.0),
        MX_ACCENT,
    );

    let inner = Rect::from_min_size(rect.min + vec2(4.0, 22.0), vec2(rect.width() - 8.0, rect.height() - 32.0));
    let mut y = inner.min.y;
    let knob_row_h = 44.0;

    // Trim
    painter.text(pos2(inner.min.x, y), egui::Align2::LEFT_TOP, "TRIM", egui::FontId::proportional(8.0), MX_DIM);
    y += 10.0;
    let trim_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), 18.0));
    let mut trim = mixer.channels[ch].trim_level as f32 / 255.0;
    let trim_resp = vertical_to_horizontal_knob(ui, painter, trim_rect, &mut trim);
    trim_resp.widget_info(|| egui::WidgetInfo::slider(true, trim as f64, format!("CH{} TRIM", ch + 1)));
    if trim_resp.dragged() {
        mixer.channels[ch].trim_level = (trim * 255.0) as u8;
        let _ = node.set_channel_trim(ch, mixer.channels[ch].trim_level);
    }
    y += knob_row_h;

    // EQ Hi
    painter.text(pos2(inner.min.x, y), egui::Align2::LEFT_TOP, "HI", egui::FontId::proportional(8.0), MX_DIM);
    y += 10.0;
    let hi_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), 18.0));
    let mut hi = mixer.channels[ch].eq_hi as f32 / 255.0;
    let hi_resp = vertical_to_horizontal_knob(ui, painter, hi_rect, &mut hi);
    hi_resp.widget_info(|| egui::WidgetInfo::slider(true, hi as f64, format!("CH{} EQ HI", ch + 1)));
    if hi_resp.dragged() {
        mixer.channels[ch].eq_hi = (hi * 255.0) as u8;
        let mid = mixer.channels[ch].eq_hi_mid;
        let low = mixer.channels[ch].eq_low;
        let _ = node.set_channel_eq(ch, mixer.channels[ch].eq_hi, mid, low);
    }
    y += knob_row_h;

    // EQ Mid
    painter.text(pos2(inner.min.x, y), egui::Align2::LEFT_TOP, "MID", egui::FontId::proportional(8.0), MX_DIM);
    y += 10.0;
    let mid_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), 18.0));
    let mut mid = mixer.channels[ch].eq_hi_mid as f32 / 255.0;
    let mid_resp = vertical_to_horizontal_knob(ui, painter, mid_rect, &mut mid);
    mid_resp.widget_info(|| egui::WidgetInfo::slider(true, mid as f64, format!("CH{} EQ MID", ch + 1)));
    if mid_resp.dragged() {
        mixer.channels[ch].eq_hi_mid = (mid * 255.0) as u8;
        let hi = mixer.channels[ch].eq_hi;
        let low = mixer.channels[ch].eq_low;
        let _ = node.set_channel_eq(ch, hi, mixer.channels[ch].eq_hi_mid, low);
    }
    y += knob_row_h;

    // EQ Low
    painter.text(pos2(inner.min.x, y), egui::Align2::LEFT_TOP, "LOW", egui::FontId::proportional(8.0), MX_DIM);
    y += 10.0;
    let low_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), 18.0));
    let mut low = mixer.channels[ch].eq_low as f32 / 255.0;
    let low_resp = vertical_to_horizontal_knob(ui, painter, low_rect, &mut low);
    low_resp.widget_info(|| egui::WidgetInfo::slider(true, low as f64, format!("CH{} EQ LOW", ch + 1)));
    if low_resp.dragged() {
        mixer.channels[ch].eq_low = (low * 255.0) as u8;
        let hi = mixer.channels[ch].eq_hi;
        let mid = mixer.channels[ch].eq_hi_mid;
        let _ = node.set_channel_eq(ch, hi, mid, mixer.channels[ch].eq_low);
    }
    y += knob_row_h;

    // Filter/Color
    painter.text(pos2(inner.min.x, y), egui::Align2::LEFT_TOP, "FILTER", egui::FontId::proportional(8.0), MX_DIM);
    y += 10.0;
    let flt_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), 18.0));
    let mut flt = mixer.channels[ch].filter_color as f32 / 255.0;
    let flt_resp = vertical_to_horizontal_knob(ui, painter, flt_rect, &mut flt);
    flt_resp.widget_info(|| egui::WidgetInfo::slider(true, flt as f64, format!("CH{} FILTER", ch + 1)));
    if flt_resp.dragged() {
        mixer.channels[ch].filter_color = (flt * 255.0) as u8;
        let _ = node.set_channel_filter(ch, mixer.channels[ch].filter_color);
    }
    y += knob_row_h;

    // Channel fader (vertical visual, horizontal interaction)
    let fader_top = y;
    let fader_rect = Rect::from_min_size(
        pos2(inner.min.x + inner.width() / 4.0, fader_top),
        vec2(inner.width() / 2.0, 80.0),
    );
    let mut fader = mixer.channels[ch].fader_level as f32 / 255.0;
    let fader_resp = vertical_fader(ui, painter, fader_rect, &mut fader);
    fader_resp.widget_info(|| egui::WidgetInfo::slider(true, fader as f64, format!("CH{} FADER", ch + 1)));
    if fader_resp.dragged() {
        mixer.channels[ch].fader_level = (fader * 255.0) as u8;
        let _ = node.set_channel_fader(ch, mixer.channels[ch].fader_level);
        // Update on_air state
        let _ = node.set_on_air(
            crate::node::tcnet_packet_serde::LayerId::from_packet_id((ch + 1) as u8).unwrap_or(crate::node::tcnet_packet_serde::LayerId::L1),
            mixer.channels[ch].fader_level,
        );
    }
    y += 88.0;

    // CUE button
    let cue_rect = Rect::from_min_size(pos2(inner.min.x, y), vec2(inner.width(), 22.0));
    let cue_color = if mixer.channels[ch].cue_a { MX_CUE_ON } else { MX_BTN };
    painter.rect_filled(cue_rect, 3.0, cue_color);
    painter.text(cue_rect.center(), egui::Align2::CENTER_CENTER, "CUE", egui::FontId::proportional(9.0), MX_TEXT);
    let cue_resp = ui.allocate_rect(cue_rect, Sense::click());
    cue_resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, mixer.channels[ch].cue_a, &format!("CH{} CUE", ch + 1)));
    if cue_resp.clicked() {
        mixer.channels[ch].cue_a = !mixer.channels[ch].cue_a;
        let _ = node.set_channel_cue(ch, mixer.channels[ch].cue_a, mixer.channels[ch].cue_b);
    }
}

fn horizontal_slider(
    ui: &mut Ui,
    painter: &egui::Painter,
    rect: Rect,
    value: &mut f32,
    color: Color32,
) -> egui::Response {
    painter.rect_filled(rect, 3.0, Color32::from_rgb(35, 35, 35));
    let handle_x = rect.min.x + *value * rect.width();
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(handle_x - rect.min.x, rect.height())),
        3.0,
        color.linear_multiply(0.3),
    );
    painter.circle_filled(pos2(handle_x, rect.center().y), 7.0, color);
    let resp = ui.allocate_rect(rect, Sense::drag());
    if resp.dragged() {
        let delta = resp.drag_delta().x / rect.width();
        *value = (*value + delta).clamp(0.0, 1.0);
    }
    resp
}

fn vertical_to_horizontal_knob(
    ui: &mut Ui,
    painter: &egui::Painter,
    rect: Rect,
    value: &mut f32,
) -> egui::Response {
    painter.rect_filled(rect, 2.0, Color32::from_rgb(35, 35, 35));
    let handle_x = rect.min.x + *value * rect.width();
    painter.circle_filled(pos2(handle_x, rect.center().y), 7.0, MX_ACCENT);
    let resp = ui.allocate_rect(rect, Sense::drag());
    if resp.dragged() {
        let delta = resp.drag_delta().x / rect.width();
        *value = (*value + delta).clamp(0.0, 1.0);
    }
    resp
}

fn vertical_fader(
    ui: &mut Ui,
    painter: &egui::Painter,
    rect: Rect,
    value: &mut f32,
) -> egui::Response {
    painter.rect_filled(rect, 3.0, Color32::from_rgb(35, 35, 35));
    // Track line in center
    let cx = rect.center().x;
    painter.line_segment(
        [pos2(cx, rect.min.y + 6.0), pos2(cx, rect.max.y - 6.0)],
        Stroke::new(2.0, MX_DIM),
    );
    // Handle position: top = max volume (1.0), bottom = zero
    let handle_y = rect.min.y + (1.0 - *value) * rect.height();
    painter.rect_filled(
        Rect::from_center_size(pos2(cx, handle_y), vec2(rect.width(), 12.0)),
        2.0,
        MX_ACCENT,
    );
    let resp = ui.allocate_rect(rect, Sense::drag());
    if resp.dragged() {
        let delta = -resp.drag_delta().y / rect.height();
        *value = (*value + delta).clamp(0.0, 1.0);
    }
    resp
}
