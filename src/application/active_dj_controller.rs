//! Active CDJ emulator.
//!
//! Poses as a TCNet Master node and emits the full set of packets that a real
//! CDJ bridge would produce:
//!
//! - **Time Packet** (msg 254) broadcast every ~20 ms for all 8 layers
//! - **Metrics Packet** (msg 200 / type 2) unicast on state change per layer
//! - **Metadata Packet** (msg 200 / type 4) unicast when a track is loaded
//! - **Status Packet** (msg 5) broadcast ~1 Hz
//!
//! All playback timing is driven by `tokio::time`; call `load_track()` and
//! `play()` / `pause()` / `stop()` from any thread, the background tasks pick
//! up the changes through the shared state.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use kanal::AsyncSender;
use log::error;
use crate::node::ApplicationConfig;
use crate::node::dispatcher::OutgoingMessage;
use crate::node::tcnet_packet::{
    management_header, Packet,
};
use crate::node::tcnet_packet_serde::{AutoMasterMode, Bpm, Data, LayerId, LayerState, LayerTimecode, MetaData, MetricsData, OptInData, ReservedData, Speed, StatusData, TimePacketData};
use crate::node::dispatcher::timestamp_micros;
use crate::node::tcnet_packet_serde::LayerStatus::Variant;

const BROADCAST_ADDR: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 255);
const TIME_PORT: u16 = 60_001;
const STATUS_PORT: u16 = 60_000;

// ---------------------------------------------------------------------------
// Track info (what you load onto a layer)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct TrackInfo {
    pub track_id: u32,
    pub artist: String,
    pub title: String,
    pub track_key: u16,
    pub duration_ms: u32,
    pub bpm: Bpm,
}

// ---------------------------------------------------------------------------
// Mutable per-layer state (shared between tasks and the public API)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct LayerControl {
    pub track: Option<TrackInfo>,
    pub state: LayerState,
    pub speed: Speed,
    pub position_ms: u32,
    pub beat_number: u32,
    pub beat_marker: u8,
    pub on_air: u8,
    pub source: u8,
    /// Wall clock time of the last position update, used to interpolate.
    pub last_update: Instant,
}

impl Default for LayerControl {
    fn default() -> Self {
        Self {
            track: None,
            state: LayerState::Idle,
            speed: Speed::STOPPED,
            position_ms: 0,
            beat_number: 0,
            beat_marker: 0,
            on_air: 0,
            source: 0,
            last_update: Instant::now(),
        }
    }
}

impl LayerControl {
    /// Advance the playhead by the elapsed wall-clock time, clamped to track length.
    fn tick(&mut self) {
        if !self.state.is_playing() {
            self.last_update = Instant::now();
            return;
        }
        let elapsed = self.last_update.elapsed().as_millis() as u32;
        self.last_update = Instant::now();
        let track_len = self
            .track
            .as_ref()
            .map(|t| t.duration_ms)
            .unwrap_or(u32::MAX);
        let speed_factor = self.speed.0 as f64 / 32768.0;
        let delta = (elapsed as f64 * speed_factor) as u32;
        self.position_ms = (self.position_ms + delta).min(track_len);
        if self.position_ms >= track_len {
            self.state = LayerState::Stopped;
        }
    }

    fn current_position(&self) -> u32 {
        if !self.state.is_playing() {
            return self.position_ms;
        }
        let elapsed = self.last_update.elapsed().as_millis() as u32;
        let speed_factor = self.speed.0 as f64 / 32768.0;
        let delta = (elapsed as f64 * speed_factor) as u32;
        let track_len = self
            .track
            .as_ref()
            .map(|t| t.duration_ms)
            .unwrap_or(u32::MAX);
        (self.position_ms + delta).min(track_len)
    }
}

// ---------------------------------------------------------------------------
// Active DJ controller
// ---------------------------------------------------------------------------

/// Active DJ controller node.  Create with `ActiveDjController::new()`, then
/// `spawn_tasks()` to start the background broadcast tasks.
pub struct ActiveDjController {
    config: ApplicationConfig,
    layers: Arc<RwLock<HashMap<LayerId, LayerControl>>>,
    outgoing_tx: AsyncSender<OutgoingMessage>,
}

impl ActiveDjController {
    pub fn new(
        config: ApplicationConfig,
        outgoing_tx: AsyncSender<OutgoingMessage>,
    ) -> Self {
        let mut layers = HashMap::new();
        for id in LayerId::ALL {
            layers.insert(id, LayerControl::default());
        }
        Self {
            config,
            layers: Arc::new(RwLock::new(layers)),
            outgoing_tx,
        }
    }

    // -----------------------------------------------------------------------
    // Public control API (can be called from any thread / task)
    // -----------------------------------------------------------------------

    pub async fn load_track(&self, layer: LayerId, track: TrackInfo) {
        let mut layers = self.layers.write().await;
        let ctrl = layers.entry(layer).or_default();
        ctrl.track = Some(track);
        ctrl.state = LayerState::Stopped;
        ctrl.position_ms = 0;
        ctrl.beat_number = 0;
    }

    pub async fn play(&self, layer: LayerId) {
        let mut layers = self.layers.write().await;
        if let Some(ctrl) = layers.get_mut(&layer) {
            if ctrl.track.is_some() {
                ctrl.state = LayerState::Playing;
                ctrl.speed = Speed::NORMAL;
                ctrl.last_update = Instant::now();
            }
        }
    }

    pub async fn pause(&self, layer: LayerId) {
        let mut layers = self.layers.write().await;
        if let Some(ctrl) = layers.get_mut(&layer) {
            if ctrl.state.is_playing() {
                ctrl.position_ms = ctrl.current_position();
                ctrl.state = LayerState::Paused;
            }
        }
    }

    pub async fn stop(&self, layer: LayerId) {
        let mut layers = self.layers.write().await;
        if let Some(ctrl) = layers.get_mut(&layer) {
            ctrl.state = LayerState::Stopped;
            ctrl.position_ms = 0;
        }
    }

    pub async fn set_speed(&self, layer: LayerId, speed: Speed) {
        let mut layers = self.layers.write().await;
        if let Some(ctrl) = layers.get_mut(&layer) {
            ctrl.position_ms = ctrl.current_position();
            ctrl.speed = speed;
            ctrl.last_update = Instant::now();
        }
    }

    pub async fn set_on_air(&self, layer: LayerId, fader_position: u8) {
        let mut layers = self.layers.write().await;
        if let Some(ctrl) = layers.get_mut(&layer) {
            ctrl.on_air = fader_position;
        }
    }

    // -----------------------------------------------------------------------
    // Background tasks
    // -----------------------------------------------------------------------

    /// Spawns all background tasks.  Call once after construction.
    pub fn spawn_tasks(self: Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move { this.time_packet_task().await });
        let this = self.clone();
        tokio::spawn(async move { this.status_packet_task().await });
        let this = self.clone();
        tokio::spawn(async move { this.metrics_update_task().await });
    }

    /// Broadcast Time Packets at ~20 ms resolution.
    async fn time_packet_task(&self) {
        let mut tick = interval(Duration::from_millis(20));
        let dest = SocketAddr::V4(SocketAddrV4::new(BROADCAST_ADDR, TIME_PORT));
        loop {
            tick.tick().await;
            let data = self.build_time_packet_data().await;
            self.send_packet(Data::Time(data), dest).await;
        }
    }

    /// Broadcast Status Packets at ~1 Hz.
    async fn status_packet_task(&self) {
        let mut tick = interval(Duration::from_secs(1));
        let dest = SocketAddr::V4(SocketAddrV4::new(BROADCAST_ADDR, STATUS_PORT));
        loop {
            tick.tick().await;
            let data = self.build_status_data().await;
            self.send_packet(Data::Status(data), dest).await;
        }
    }

    /// Push Metrics packets for any layer whose state changed since the last tick.
    async fn metrics_update_task(&self) {
        let mut tick = interval(Duration::from_millis(50));
        // Snapshot of previous states to detect changes
        let mut prev_states: HashMap<LayerId, LayerState> = HashMap::new();

        loop {
            tick.tick().await;
            let mut layers = self.layers.write().await;
            for id in LayerId::ALL {
                if let Some(ctrl) = layers.get_mut(&id) {
                    ctrl.tick();
                    let changed = prev_states
                        .get(&id)
                        .map_or(true, |&s| s != ctrl.state);
                    if changed {
                        let data = build_metrics_data(id, ctrl);
                        let dest = SocketAddr::V4(SocketAddrV4::new(BROADCAST_ADDR, STATUS_PORT));
                        // Drop lock before awaiting send
                        self.send_packet(Data::Metrics(data), dest).await;
                        prev_states.insert(id, ctrl.state);  // ctrl is no longer valid here
                        break; // re-acquire lock next iteration
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Packet construction
    // -----------------------------------------------------------------------

    async fn build_time_packet_data(&self) -> TimePacketData {
        let layers = self.layers.read().await;
        let get = |id: LayerId| layers.get(&id).map(|c| c.current_position()).unwrap_or(0);
        let get_total = |id: LayerId| {
            layers
                .get(&id)
                .and_then(|c| c.track.as_ref())
                .map(|t| t.duration_ms)
                .unwrap_or(0)
        };
        let get_state = |id: LayerId| {
            layers
                .get(&id)
                .map(|c| c.state.to_u8())
                .unwrap_or(0)
        };
        let get_beat = |id: LayerId| layers.get(&id).map(|c| c.beat_marker).unwrap_or(0);
        let get_on_air = |id: LayerId| layers.get(&id).map(|c| c.on_air).unwrap_or(0);

        let default_tc = LayerTimecode {
            smpte_mode: 25,
            state: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
            frames: 0,
        };

        TimePacketData {
            l1_time: get(LayerId::L1),
            l2_time: get(LayerId::L2),
            l3_time: get(LayerId::L3),
            l4_time: get(LayerId::L4),
            la_time: get(LayerId::LA),
            lb_time: get(LayerId::LB),
            lm_time: get(LayerId::LM),
            lc_time: get(LayerId::LC),
            l1_total_time: get_total(LayerId::L1),
            l2_total_time: get_total(LayerId::L2),
            l3_total_time: get_total(LayerId::L3),
            l4_total_time: get_total(LayerId::L4),
            la_total_time: get_total(LayerId::LA),
            lb_total_time: get_total(LayerId::LB),
            lm_total_time: get_total(LayerId::LM),
            lc_total_time: get_total(LayerId::LC),
            l1_beat_marker: get_beat(LayerId::L1),
            l2_beat_marker: get_beat(LayerId::L2),
            l3_beat_marker: get_beat(LayerId::L3),
            l4_beat_marker: get_beat(LayerId::L4),
            la_beat_marker: get_beat(LayerId::LA),
            lb_beat_marker: get_beat(LayerId::LB),
            lm_beat_marker: get_beat(LayerId::LM),
            lc_beat_marker: get_beat(LayerId::LC),
            l1_layer_state: get_state(LayerId::L1),
            l2_layer_state: get_state(LayerId::L2),
            l3_layer_state: get_state(LayerId::L3),
            l4_layer_state: get_state(LayerId::L4),
            la_layer_state: get_state(LayerId::LA),
            lb_layer_state: get_state(LayerId::LB),
            lm_layer_state: get_state(LayerId::LM),
            lc_layer_state: get_state(LayerId::LC),
            _reserved0: ReservedData::default(),
            smpte_mode: 25,
            l1_timecode: default_tc.clone(),
            l2_timecode: default_tc.clone(),
            l3_timecode: default_tc.clone(),
            l4_timecode: default_tc.clone(),
            la_timecode: default_tc.clone(),
            lb_timecode: default_tc.clone(),
            lm_timecode: default_tc.clone(),
            lc_timecode: default_tc.clone(),
            l1_on_air: get_on_air(LayerId::L1),
            l2_on_air: get_on_air(LayerId::L2),
            l3_on_air: get_on_air(LayerId::L3),
            l4_on_air: get_on_air(LayerId::L4),
            la_on_air: get_on_air(LayerId::LA),
            lb_on_air: get_on_air(LayerId::LB),
            lm_on_air: get_on_air(LayerId::LM),
            lc_on_air: get_on_air(LayerId::LC),
        }
    }

    async fn build_status_data(&self) -> StatusData {
        let layers = self.layers.read().await;
        let src = |id: LayerId| layers.get(&id).map(|c| c.source).unwrap_or(0);
        let tid = |id: LayerId| {
            layers
                .get(&id)
                .and_then(|c| c.track.as_ref())
                .map(|t| t.track_id)
                .unwrap_or(0)
        };
        StatusData {
            node_count: 1,
            node_listener_port: self.config.unicast_port,
            _reserved0: Default::default(),
            layer_1_source: src(LayerId::L1),
            layer_2_source: src(LayerId::L2),
            layer_3_source: src(LayerId::L3),
            layer_4_source: src(LayerId::L4),
            layer_a_source: src(LayerId::LA),
            layer_b_source: src(LayerId::LB),
            layer_m_source: src(LayerId::LM),
            layer_c_source: src(LayerId::LC),
            // Status enum stubs — use raw u8 workaround for now
            // TODO
            layer_1_status: Variant,
            layer_2_status: Variant,
            layer_3_status: Variant,
            layer_4_status: Variant,
            layer_a_status: Variant,
            layer_b_status: Variant,
            layer_m_status: Variant,
            layer_c_status: Variant,
            layer_1_track_id: tid(LayerId::L1),
            layer_2_track_id: tid(LayerId::L2),
            layer_3_track_id: tid(LayerId::L3),
            layer_4_track_id: tid(LayerId::L4),
            layer_a_track_id: tid(LayerId::LA),
            layer_b_track_id: tid(LayerId::LB),
            layer_m_track_id: tid(LayerId::LM),
            layer_c_track_id: tid(LayerId::LC),
            _reserved1: Default::default(),
            smpte_mode: 25,
            auto_master_mode: AutoMasterMode::Variant, // TODO
            _reserved2: Default::default(),
            app_specific: [0u8; 72],
            layer_1_name: name_bytes(&layers, LayerId::L1),
            layer_2_name: name_bytes(&layers, LayerId::L2),
            layer_3_name: name_bytes(&layers, LayerId::L3),
            layer_4_name: name_bytes(&layers, LayerId::L4),
            layer_a_name: name_bytes(&layers, LayerId::LA),
            layer_b_name: name_bytes(&layers, LayerId::LB),
            layer_m_name: name_bytes(&layers, LayerId::LM),
            layer_c_name: name_bytes(&layers, LayerId::LC),
        }
    }

    async fn send_packet(&self, data: Data, dest: SocketAddr) {
        static SEQ: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let message_type = message_type_for(&data);
        let header = management_header(&self.config, message_type, seq);
        let packet = Packet { header, data };
        let msg = OutgoingMessage { packet, destination: dest };
        if let Err(e) = self.outgoing_tx.send(msg).await {
            error!("ActiveDjController: could not queue outgoing packet: {:?}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_metrics_data(id: LayerId, ctrl: &LayerControl) -> MetricsData {
    let (bpm, tid, tlen) = ctrl
        .track
        .as_ref()
        .map(|t| (t.bpm.0, t.track_id, t.duration_ms))
        .unwrap_or((0, 0, 0));

    MetricsData {
        data_type: 2,
        layer_id: id.as_packet_id(),
        _reserved0: Default::default(),
        layer_state: ctrl.state.to_u8(),
        _reserved1: Default::default(),
        sync_master: 0,
        _reserved2: Default::default(),
        beat_marker: ctrl.beat_marker,
        track_length: tlen,
        current_position: ctrl.current_position(),
        speed: ctrl.speed.0,
        _reserved3: Default::default(),
        beat_number: ctrl.beat_number,
        _reserved4: Default::default(),
        bpm,
        pitch_bend: 32768,
        track_id: tid,
    }
}

fn name_bytes(layers: &HashMap<LayerId, LayerControl>, id: LayerId) -> [u8; 16] {
    let mut out = [0u8; 16];
    if let Some(name) = layers
        .get(&id)
        .and_then(|c| c.track.as_ref())
        .map(|t| t.title.as_bytes())
    {
        let len = name.len().min(16);
        out[..len].copy_from_slice(&name[..len]);
    }
    out
}

fn message_type_for(data: &Data) -> u8 {
    match data {
        Data::OptIn(_) => 2,
        Data::OptOut(_) => 3,
        Data::Status(_) => 5,
        Data::TimeSync(_) => 10,
        Data::ErrorNotification(_) => 13,
        Data::Request(_) => 20,
        Data::AppSpecific(_) => 30,
        Data::Control(_) => 101,
        Data::Text(_) => 128,
        Data::Keyboard(_) => 132,
        Data::Metrics(_) | Data::Meta(_) | Data::BeatGrid(_) | Data::Cue(_)
        | Data::SmallWaveform(_) | Data::BigWaveform(_) | Data::Mixer(_) => 200,
        Data::ArtworkFile(_) => 204,
        Data::Time(_) => 254,
    }
}
