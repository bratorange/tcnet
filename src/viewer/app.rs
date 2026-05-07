use std::time::Duration;
use egui::Color32;
use crate::{DjControllerView, TCNetClient};
use crate::node::tcnet_packet_serde::LayerId;
use super::waveform::WaveformCache;
use super::ui::{deck_header, waveform_lane};

const BG: Color32 = Color32::from_rgb(10, 10, 10);
const HEADER_H: f32 = 100.0;
const LANE_H: f32 = 130.0;

enum AppState {
    Waiting { client: TCNetClient },
    Viewing { _client: TCNetClient, view: DjControllerView, cache: WaveformCache },
}

pub struct ViewerApp {
    state: Option<AppState>,
}

impl ViewerApp {
    pub fn new(client: TCNetClient) -> Self {
        Self { state: Some(AppState::Waiting { client }) }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(33));

        // Try to acquire a controller view if still waiting
        if matches!(self.state, Some(AppState::Waiting { .. })) {
            if let Some(AppState::Waiting { client }) = self.state.take() {
                match client.get_any_controller_view() {
                    Some(view) => {
                        self.state = Some(AppState::Viewing {
                            _client: client,
                            view,
                            cache: WaveformCache::new(),
                        });
                    }
                    None => {
                        self.state = Some(AppState::Waiting { client });
                    }
                }
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG))
            .show(ctx, |ui| {
                match &mut self.state {
                    Some(AppState::Waiting { .. }) => {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new("No controller detected")
                                    .color(Color32::from_rgb(80, 80, 80))
                                    .size(22.0),
                            );
                        });
                    }
                    Some(AppState::Viewing { view, cache, .. }) => {
                        cache.poll();

                        // Clone snapshot so borrow on `view` ends before we use `cache`
                        let layers: Vec<_> = view.get_layers().to_vec();
                        let requester = view.waveform_requester();

                        const LAYER_IDS: [LayerId; 4] =
                            [LayerId::L1, LayerId::L2, LayerId::L3, LayerId::L4];

                        for (i, (&layer_id, layer)) in
                            LAYER_IDS.iter().zip(layers.iter()).enumerate()
                        {
                            cache.update(i, layer.track_id, layer_id, &requester);
                        }

                        let avail_w = ui.available_width();

                        // 2×2 header grid
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                            deck_header::show(ui, 1, &layers[0], cache.small[0].as_deref(), avail_w / 2.0, HEADER_H);
                            deck_header::show(ui, 2, &layers[1], cache.small[1].as_deref(), avail_w / 2.0, HEADER_H);
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                            deck_header::show(ui, 3, &layers[2], cache.small[2].as_deref(), avail_w / 2.0, HEADER_H);
                            deck_header::show(ui, 4, &layers[3], cache.small[3].as_deref(), avail_w / 2.0, HEADER_H);
                        });

                        // 4 stacked waveform lanes
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        for (i, layer) in layers[..4].iter().enumerate() {
                            waveform_lane::show(
                                ui,
                                i + 1,
                                layer,
                                cache.small[i].as_deref(),
                                avail_w,
                                LANE_H,
                            );
                        }
                    }
                    None => {}
                }
            });
    }
}
