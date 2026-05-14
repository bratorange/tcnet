use egui::{Color32, Vec2, vec2};
use crate::active_node::ActiveDJNode;
use crate::node::dj_controller::MixerSnapshot;
use crate::node::tcnet_packet_serde::LayerId;
use crate::simulator::audio::AudioEngine;
use crate::simulator::cdj_deck::CDJDeck;
use crate::simulator::virtual_usb::{TrackInfo, VirtualUsb};
use crate::simulator::ui::{cdj_panel, mixer_panel};
use crate::simulator::mcp::{SimBridge, SimCmd};
use std::sync::{Arc, Mutex};
use egui_mcp_client::McpClient;
use image::{ImageBuffer, Rgba, ImageFormat};

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
    bridge: Option<Arc<Mutex<SimBridge>>>,
    mcp_client: McpClient,
    rt: tokio::runtime::Runtime,
}

impl SimulatorApp {
    pub fn new(node: ActiveDJNode, usb: VirtualUsb, audio: AudioEngine, bridge: Option<Arc<Mutex<SimBridge>>>, mcp_client: McpClient, rt: tokio::runtime::Runtime) -> Self {
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
            bridge,
            mcp_client,
            rt,
        }
    }

    fn process_bridge_commands(&mut self) {
        let commands: Vec<SimCmd> = if let Some(ref bridge) = self.bridge {
            if let Ok(mut b) = bridge.lock() {
                b.commands.drain(..).collect()
            } else {
                return;
            }
        } else {
            return;
        };

        for cmd in commands {
            match cmd {
                SimCmd::Play(deck) => {
                    if deck == 1 { self.deck1.play(&mut self.node); }
                    else if deck == 2 { self.deck2.play(&mut self.node); }
                }
                SimCmd::Pause(deck) => {
                    if deck == 1 { self.deck1.pause(&mut self.node); }
                    else if deck == 2 { self.deck2.pause(&mut self.node); }
                }
                SimCmd::Stop(deck) => {
                    if deck == 1 { self.deck1.stop(&mut self.node); }
                    else if deck == 2 { self.deck2.stop(&mut self.node); }
                }
                SimCmd::LoadTrack { deck, filter } => {
                    let filter_lc = filter.to_lowercase();
                    if let Some(track) = self.usb.tracks.iter()
                        .find(|t| t.title.to_lowercase().contains(&filter_lc)
                            || t.artist.to_lowercase().contains(&filter_lc))
                        .cloned()
                    {
                        if deck == 1 { self.deck1.load(track, &self.audio, &mut self.node); }
                        else if deck == 2 { self.deck2.load(track, &self.audio, &mut self.node); }
                    }
                }
                SimCmd::SetCrossfader(value) => {
                    self.mixer.crossfader = value;
                    let _ = self.node.set_crossfader(value);
                }
            }
        }
    }

    fn update_bridge_state(&self) {
        if let Some(ref bridge) = self.bridge {
            if let Ok(mut b) = bridge.lock() {
                b.deck1.title = self.deck1.loaded_track.as_ref()
                    .map(|t| t.title.clone()).unwrap_or_default();
                b.deck1.artist = self.deck1.loaded_track.as_ref()
                    .map(|t| t.artist.clone()).unwrap_or_default();
                b.deck1.bpm = self.deck1.bpm;
                b.deck1.position_ms = self.deck1.current_position_ms();
                b.deck1.duration_ms = self.deck1.duration_ms();
                b.deck1.is_playing = self.deck1.is_playing();
                b.deck1.is_loaded = self.deck1.loaded_track.is_some();

                b.deck2.title = self.deck2.loaded_track.as_ref()
                    .map(|t| t.title.clone()).unwrap_or_default();
                b.deck2.artist = self.deck2.loaded_track.as_ref()
                    .map(|t| t.artist.clone()).unwrap_or_default();
                b.deck2.bpm = self.deck2.bpm;
                b.deck2.position_ms = self.deck2.current_position_ms();
                b.deck2.duration_ms = self.deck2.duration_ms();
                b.deck2.is_playing = self.deck2.is_playing();
                b.deck2.is_loaded = self.deck2.loaded_track.is_some();

                b.crossfader = self.mixer.crossfader;
                b.available_tracks = self.usb.tracks.iter()
                    .map(|t| format!("{} — {}", t.title, t.artist))
                    .collect();
            }
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
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let inputs = self.rt.block_on(self.mcp_client.take_pending_inputs());
        egui_mcp_client::inject_inputs(ctx, raw_input, inputs);

        // Deliver any screenshot that eframe rendered last frame
        for event in &raw_input.events {
            if let egui::Event::Screenshot { image, .. } = event {
                let w = image.width() as u32;
                let h = image.height() as u32;
                let rgba_bytes: Vec<u8> = image.pixels.iter()
                    .flat_map(|c| [c.r(), c.g(), c.b(), c.a()])
                    .collect();
                if let Some(img) = ImageBuffer::<Rgba<u8>, _>::from_raw(w, h, rgba_bytes) {
                    let mut png = Vec::new();
                    let _ = img.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png);
                    let client = self.mcp_client.clone();
                    self.rt.block_on(client.set_screenshot(png));
                }
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        // If the MCP server requested a screenshot, ask eframe for one
        if self.rt.block_on(self.mcp_client.take_screenshot_request()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }

        self.process_bridge_commands();
        self.deck1.tick(&mut self.node);
        self.deck2.tick(&mut self.node);
        self.update_audio_volumes();
        self.update_bridge_state();

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

        let highlights = self.rt.block_on(self.mcp_client.get_highlights());
        egui_mcp_client::draw_highlights(ctx, &highlights);
        let _ = self.rt.block_on(self.mcp_client.record_frame_auto());
    }
}
