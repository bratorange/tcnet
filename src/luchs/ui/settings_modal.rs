use egui::{Color32, Ui};

use crate::luchs::settings::LuchsConfig;

pub struct SettingsModalResult {
    pub close: bool,
    pub save: bool,
}

/// Render the settings modal. Mutates `config` in place; the caller decides
/// when to persist (typically on Save click).
pub fn show(ctx: &egui::Context, open: &mut bool, config: &mut LuchsConfig) -> SettingsModalResult {
    let mut close = false;
    let mut save = false;

    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(true)
        .default_width(420.0)
        .open(open)
        .show(ctx, |ui| {
            ui.heading("OSC");
            ui.separator();

            ui.label("Endpoints (host:port, one per line):");
            let mut joined = config.osc_endpoints.join("\n");
            let resp = ui.add(
                egui::TextEdit::multiline(&mut joined)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("127.0.0.1:9000\n10.0.0.5:9001"),
            );
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::TextEdit,
                    true,
                    "OSC endpoints",
                )
            });
            if resp.changed() {
                config.osc_endpoints = joined
                    .split('\n')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }

            // Hint about parse status.
            let bad: Vec<&String> = config
                .osc_endpoints
                .iter()
                .filter(|s| s.parse::<std::net::SocketAddr>().is_err())
                .collect();
            if !bad.is_empty() {
                ui.colored_label(
                    Color32::from_rgb(0xE0, 0x80, 0x40),
                    format!("Couldn't parse: {}", bad.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                );
            }

            ui.add_space(8.0);
            address_row(ui, "Phrase address:", &mut config.phrase_address);
            address_row(ui, "Beat address:", &mut config.beat_address);
            ui.checkbox(
                &mut config.forward_all_decks,
                "Forward events for all decks (not just on-air)",
            );

            ui.add_space(12.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
                ui.label(
                    egui::RichText::new(format!(
                        "Config: {}",
                        LuchsConfig::config_path().display()
                    ))
                    .small()
                    .color(Color32::from_rgb(0x80, 0x80, 0x88)),
                );
            });
        });

    SettingsModalResult { close, save }
}

fn address_row(ui: &mut Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        let resp = ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(220.0),
        );
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, label)
        });
    });
}
