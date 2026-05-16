use egui::{Color32, Sense, Stroke, Ui, Vec2};

use super::palette::{
    CONNECTED_GREEN, NEXT_BLUE, ON_AIR_RED, RECONNECT_AMBER, TEXT_DIM, TEXT_PRIMARY, TOP_BAR_BG,
    TOP_BAR_BORDER,
};

pub struct TopBarInput<'a> {
    pub connected: bool,
    pub tcnet_ip: &'a str,
    pub player_count: usize,
    pub on_air_deck: Option<u8>,
    pub next_deck: Option<u8>,
}

pub struct TopBarOutput {
    pub settings_clicked: bool,
}

pub fn show(ui: &mut Ui, input: TopBarInput<'_>) -> TopBarOutput {
    let bar_h = 32.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), bar_h), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, TOP_BAR_BG);
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.bottom() - 0.5),
            egui::pos2(rect.right(), rect.bottom() - 0.5),
        ],
        Stroke::new(0.5, TOP_BAR_BORDER),
    );

    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(12.0, 0.0))));
    child.horizontal_centered(|ui| {
        ui.label(
            egui::RichText::new("LUCHS")
                .color(TEXT_PRIMARY)
                .strong()
                .size(13.0),
        );
        ui.label(
            egui::RichText::new("— annotator")
                .color(TEXT_DIM)
                .size(12.0),
        );

        ui.add_space(20.0);

        connection_indicator(ui, input.connected);
        let label_text = if input.connected {
            format!("TCNet {} ({})", input.tcnet_ip, input.player_count)
        } else {
            "TCNet — reconnecting".to_string()
        };
        ui.label(
            egui::RichText::new(label_text)
                .color(TEXT_PRIMARY)
                .size(12.0)
                .monospace(),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = ui.add(egui::Button::new(
                egui::RichText::new("⚙").size(14.0).color(TEXT_PRIMARY),
            ).frame(false).min_size(Vec2::new(24.0, 24.0)));
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Settings")
            });
            let settings_clicked = resp.clicked();

            ui.add_space(8.0);

            if let Some(deck) = input.next_deck {
                pill(ui, &format!("▷ NEXT: DECK-{}", deck), NEXT_BLUE);
            }
            if let Some(deck) = input.on_air_deck {
                pill(ui, &format!("▶ ON AIR: DECK-{}", deck), ON_AIR_RED);
            }

            ui.memory_mut(|m| m.data.insert_temp(egui::Id::new("luchs_settings_clicked"), settings_clicked));
        });
    });

    let settings_clicked = ui
        .memory_mut(|m| m.data.get_temp::<bool>(egui::Id::new("luchs_settings_clicked")))
        .unwrap_or(false);

    TopBarOutput { settings_clicked }
}

fn connection_indicator(ui: &mut Ui, connected: bool) {
    let radius = 4.5;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(radius * 2.0 + 4.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let base_color = if connected { CONNECTED_GREEN } else { RECONNECT_AMBER };
    let alpha = if connected {
        let t = ui.input(|i| i.time);
        let phase = ((t * std::f64::consts::PI).sin() * 0.5 + 0.5) as f32;
        0.4 + 0.6 * phase
    } else {
        1.0
    };
    let color = with_alpha(base_color, alpha);
    painter.circle_filled(center, radius, color);
}

fn pill(ui: &mut Ui, label: &str, color: Color32) {
    let padding_x = 8.0;
    let padding_y = 3.0;
    let text = egui::RichText::new(label).color(Color32::WHITE).size(11.0).strong();
    let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(11.0), Color32::WHITE);
    let _ = text;
    let size = Vec2::new(galley.size().x + padding_x * 2.0, galley.size().y + padding_y * 2.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, color);
    painter.galley(
        rect.left_top() + Vec2::new(padding_x, padding_y),
        galley,
        Color32::WHITE,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
    ui.add_space(6.0);
}

fn with_alpha(c: Color32, a: f32) -> Color32 {
    let a = (a.clamp(0.0, 1.0) * 255.0) as u8;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
