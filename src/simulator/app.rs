use egui::{Color32, Vec2, vec2};
use crate::active_node::ActiveDJNode;
use crate::node::dj_controller::MixerSnapshot;
use crate::node::tcnet_packet_serde::LayerId;
use crate::simulator::audio::AudioEngine;
use crate::simulator::cdj_deck::CDJDeck;
use crate::simulator::virtual_usb::{TrackInfo, VirtualUsb};
use crate::simulator::ui::{cdj_panel, mixer_panel};

pub struct SimulatorApp {
    deck1: CDJDeck,
    deck2: CDJDeck,
    mixer: MixerSnapshot,
    node: ActiveDJNode,
    audio: AudioEngine,
    usb: VirtualUsb,
    show_browser: bool,
    browser_target: u8,   // 1 = deck1, 2 = deck2
    browser_filter: String,
}

impl SimulatorApp {
    pub fn new(node: ActiveDJNode, usb: VirtualUsb, audio: AudioEngine) -> Self {
        let mut mixer = MixerSnapshot::default();
        mixer.mixer_name = "DJM-A9".to_string();
        for ch in &mut mixer.channels {
            ch.fader_level = 200;
            ch.eq_hi = 128;
            ch.eq_hi_mid = 128;
            ch.eq_low = 128;
        }
        mixer.master_fader_level = 200;
        mixer.crossfader = 128;

        Self {
            deck1: CDJDeck::new(LayerId::L1),
            deck2: CDJDeck::new(LayerId::L2),
            mixer,
            node,
            audio,
            usb,
            show_browser: false,
            browser_target: 1,
            browser_filter: String::new(),
        }
    }

    fn update_audio_volumes(&self) {
        let xf = self.mixer.crossfader as f32 / 255.0;
        let master = self.mixer.master_fader_level as f32 / 255.0;
        let ch1_fader = self.mixer.channels[0].fader_level as f32 / 255.0;
        let ch2_fader = self.mixer.channels[1].fader_level as f32 / 255.0;
        // Linear crossfader: left (ch1) fades out as xf goes right
        let xf_vol1 = (1.0 - xf).clamp(0.0, 1.0);
        let xf_vol2 = xf.clamp(0.0, 1.0);
        self.deck1.set_volume(ch1_fader * xf_vol1 * master);
        self.deck2.set_volume(ch2_fader * xf_vol2 * master);
    }

    fn draw_browser_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Track Browser")
            .resizable(true)
            .default_size(vec2(500.0, 400.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.browser_filter);
                    if ui.button("Refresh").clicked() {
                        self.usb.scan();
                    }
                    if ui.button("Close").clicked() {
                        self.show_browser = false;
                    }
                });
                ui.separator();

                let filter = self.browser_filter.to_lowercase();
                let tracks: Vec<TrackInfo> = self.usb.tracks.iter()
                    .filter(|t| {
                        filter.is_empty()
                            || t.title.to_lowercase().contains(&filter)
                            || t.artist.to_lowercase().contains(&filter)
                    })
                    .cloned()
                    .collect();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for track in tracks {
                        ui.horizontal(|ui| {
                            let dur = CDJDeck::format_time(track.duration_ms);
                            let label = format!(
                                "{} — {} ({})",
                                track.title, track.artist, dur
                            );
                            let resp = ui.selectable_label(false, &label);
                            if resp.double_clicked() {
                                let target = self.browser_target;
                                if target == 1 {
                                    self.deck1.load(track, &self.audio, &mut self.node);
                                } else {
                                    self.deck2.load(track, &self.audio, &mut self.node);
                                }
                                self.show_browser = false;
                            }
                        });
                    }
                });
            });
    }
}

impl eframe::App for SimulatorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        self.deck1.tick(&mut self.node);
        self.deck2.tick(&mut self.node);
        self.update_audio_volumes();

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(18, 18, 18)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(8.0);

                    let r1 = cdj_panel::show(ui, &mut self.deck1, &mut self.node, "CDJ  #1");
                    if r1.open_browser {
                        self.browser_target = 1;
                        self.show_browser = true;
                    }

                    mixer_panel::show(ui, &mut self.mixer, &mut self.node);

                    let r2 = cdj_panel::show(ui, &mut self.deck2, &mut self.node, "CDJ  #2");
                    if r2.open_browser {
                        self.browser_target = 2;
                        self.show_browser = true;
                    }
                });
            });

        if self.show_browser {
            self.draw_browser_window(ctx);
        }
    }
}
