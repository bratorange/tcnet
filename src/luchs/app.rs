use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::Color32;
use egui_mcp_client::McpClient;
use egui_mcp_protocol::UiTree;
use image::{ImageBuffer, ImageFormat, Rgba};

use crate::media_library::VirtualUsb;
use crate::{DjControllerView, TCNetClient};

use super::analysis::AnalysisManager;
use super::osc::{OscConfig, OscSender};
use super::settings::LuchsConfig;
use super::state::LuchsState;
use super::ui::needle_lane;
use super::ui::overview_deck;
use super::ui::palette::APP_BG;
use super::ui::settings_modal;
use super::ui::status_bar;
use super::ui::top_bar;
use super::waveform_pull::WaveformPuller;

/// egui plugin that captures the AccessKit tree from `output_hook`, which runs
/// after `end_pass()` populates `platform_output.accesskit_update`.
struct AccessKitCapturePlugin {
    pending: Arc<Mutex<Option<UiTree>>>,
}

impl egui::Plugin for AccessKitCapturePlugin {
    fn debug_name(&self) -> &'static str {
        "AccessKitCapture"
    }

    fn output_hook(&mut self, output: &mut egui::FullOutput) {
        if let Some(tree) = &output.platform_output.accesskit_update {
            let ui_tree = super::accesskit_tree::convert(tree);
            if let Ok(mut guard) = self.pending.lock() {
                *guard = Some(ui_tree);
            }
        }
    }
}

pub struct LuchsApp {
    client: TCNetClient,
    view: Option<DjControllerView>,
    state: LuchsState,
    waveform_puller: WaveformPuller,
    analysis: AnalysisManager,
    library: VirtualUsb,
    bind_ip_label: String,
    mcp_client: McpClient,
    rt: tokio::runtime::Runtime,
    pending_tree: Arc<Mutex<Option<UiTree>>>,
    plugin_registered: bool,

    config: LuchsConfig,
    osc_config: OscConfig,
    osc_sender: OscSender,
    show_settings: bool,
}

impl LuchsApp {
    pub fn new(
        client: TCNetClient,
        bind_ip_label: String,
        media_dir: PathBuf,
        script_dir: PathBuf,
        mcp_client: McpClient,
        rt: tokio::runtime::Runtime,
    ) -> Self {
        let waveform_puller = WaveformPuller::new(client.runtime_handle());
        let analysis = AnalysisManager::new(script_dir);
        let library = VirtualUsb::from_dir(media_dir.clone());
        log::info!(
            "luchs media library indexed: {} tracks under {:?}",
            library.tracks.len(),
            library.root
        );
        let config = LuchsConfig::load();
        let osc_config = OscConfig::new();
        osc_config.update(
            config.parsed_endpoints(),
            config.phrase_address.clone(),
            config.beat_address.clone(),
            config.forward_all_decks,
        );
        let osc_sender = OscSender::new(osc_config.clone());

        Self {
            client,
            view: None,
            state: LuchsState::default(),
            waveform_puller,
            analysis,
            library,
            bind_ip_label,
            mcp_client,
            rt,
            pending_tree: Arc::new(Mutex::new(None)),
            plugin_registered: false,
            config,
            osc_config,
            osc_sender,
            show_settings: false,
        }
    }

    fn refresh_view(&mut self) {
        if self.view.is_none() {
            self.view = self.client.get_any_controller_view();
        }
        // Drop the view if no foreign node still publishes a DJ controller.
        let any_controller = self
            .client
            .active_nodes()
            .iter()
            .any(|n| n.has_dj_controller);
        if !any_controller {
            self.view = None;
            self.state = LuchsState::default();
        }

        let nodes_with_ctrl = self
            .client
            .active_nodes()
            .iter()
            .filter(|n| n.has_dj_controller)
            .count();
        self.state.connected = nodes_with_ctrl > 0;
        self.state.player_count = nodes_with_ctrl;

        if let Some(view) = self.view.as_mut() {
            self.state.refresh(
                view,
                &mut self.waveform_puller,
                &mut self.analysis,
                &self.library,
                &self.osc_sender,
                self.config.forward_all_decks,
            );
        }
    }
}

impl eframe::App for LuchsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        if !self.plugin_registered {
            ctx.add_plugin(AccessKitCapturePlugin {
                pending: self.pending_tree.clone(),
            });
            self.plugin_registered = true;
        }

        if let Ok(mut guard) = self.pending_tree.lock() {
            if let Some(tree) = guard.take() {
                let client = self.mcp_client.clone();
                let _ = self.rt.block_on(client.set_ui_tree(tree));
            }
        }

        if self.rt.block_on(self.mcp_client.take_screenshot_request()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }

        self.refresh_view();

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(APP_BG))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 0.0);

                let bar = top_bar::show(
                    ui,
                    top_bar::TopBarInput {
                        connected: self.state.connected,
                        tcnet_ip: &self.bind_ip_label,
                        player_count: self.state.player_count,
                        on_air_deck: self.state.on_air_deck(),
                        next_deck: self.state.next_deck(),
                    },
                );
                if bar.settings_clicked {
                    self.show_settings = !self.show_settings;
                }

                ui.add_space(6.0);

                if !self.state.connected {
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(ui.available_width(), ui.available_height()),
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            ui.label(
                                egui::RichText::new("waiting for TCNet...")
                                    .color(Color32::from_rgb(0x55, 0x55, 0x5C))
                                    .size(20.0),
                            );
                        },
                    );
                    return;
                }

                draw_overview_grid(ui, &self.state);
                ui.add_space(8.0);
                draw_needle_stack(ui, &self.state);

                // Push the status bar to the bottom.
                let remaining = ui.available_height();
                if remaining > 0.0 {
                    ui.add_space(remaining - 26.0);
                }
                status_bar::show(ui, &self.state);
            });

        if self.show_settings {
            let mut open = self.show_settings;
            let result = settings_modal::show(ctx, &mut open, &mut self.config);
            self.show_settings = open;
            if result.close {
                self.show_settings = false;
            }
            if result.save {
                if let Err(e) = self.config.save() {
                    log::warn!("luchs config save failed: {}", e);
                }
                self.osc_config.update(
                    self.config.parsed_endpoints(),
                    self.config.phrase_address.clone(),
                    self.config.beat_address.clone(),
                    self.config.forward_all_decks,
                );
            }
        }

        let highlights = self.rt.block_on(self.mcp_client.get_highlights());
        egui_mcp_client::draw_highlights(ctx, &highlights);
        let _ = self.rt.block_on(self.mcp_client.record_frame_auto());
    }
}
impl LuchsApp{
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let inputs = self.rt.block_on(self.mcp_client.take_pending_inputs());
        egui_mcp_client::inject_inputs(ctx, raw_input, inputs);

        for event in &raw_input.events {
            if let egui::Event::Screenshot { image, .. } = event {
                let w = image.width() as u32;
                let h = image.height() as u32;
                let rgba_bytes: Vec<u8> = image
                    .pixels
                    .iter()
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
}

fn draw_overview_grid(ui: &mut egui::Ui, state: &LuchsState) {
    let avail = ui.available_size_before_wrap();
    let gap = 6.0;
    let col_w = (avail.x - gap) / 2.0;
    // The 2x2 overview occupies roughly the top half — leaves room for needle
    // lanes (4 × 64px + spacing) + status bar.
    let total_overview_h = (avail.y * 0.5).clamp(220.0, 360.0);
    let row_h = (total_overview_h - gap) / 2.0;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::splat(gap);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, gap);
            overview_deck::show(ui, &state.decks[0], egui::Vec2::new(col_w, row_h));
            overview_deck::show(ui, &state.decks[2], egui::Vec2::new(col_w, row_h));
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, gap);
            overview_deck::show(ui, &state.decks[1], egui::Vec2::new(col_w, row_h));
            overview_deck::show(ui, &state.decks[3], egui::Vec2::new(col_w, row_h));
        });
    });
}

fn draw_needle_stack(ui: &mut egui::Ui, state: &LuchsState) {
    let lane_h = 64.0;
    ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 2.0);
    for deck in &state.decks {
        needle_lane::show(ui, deck, lane_h);
    }
}
