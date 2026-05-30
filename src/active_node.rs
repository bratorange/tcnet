//! Active broadcaster role — internal broadcaster handle that
//! [`Node<Master, V>`](crate::api::Node) derefs to.
//!
//! Owns per-layer + mixer state via per-field `ArcSwap`, drives the
//! periodic Time / Status / Metrics / Meta / Mixer emission loop,
//! serves request/response from a pre-populated response cache.
//! Surfaced through `Node<Master>` so callers see one typed handle;
//! see [`crate::api`] for the public API.

use crate::node::dj_controller::{ChannelSnapshot, LayerSnapshot, MixerSnapshot};
use crate::node::response_data::SharedResponseData;
use crate::node::tcnet_packet::Data;
use crate::protocol::{
    ArtworkFileData, AutoMasterMode, BeatGridEntry, BeatGridHeader, BigWaveformData, Bpm, CueData,
    CueEntry, LayerId, LayerState, LayerStatus, LayerTimecode, MetaData, MetricsData, MixerChannel,
    MixerData, ReservedData, SmallWaveformData, SmpteMode, Speed, StatusData, TimePacketData,
};
use arc_swap::ArcSwap;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::interval;

pub use kanal::SendError;

// ---------------------------------------------------------------------------
// Public metadata type for track loading
// ---------------------------------------------------------------------------

/// Minimum metadata required to "load" a track on an [`ActiveDJNode`] layer.
///
/// Passed to [`ActiveDJNode::load_track`] — populates the layer's
/// [`StatusData`] / [`MetaData`] fields and pre-builds a synthesised beat grid
/// from `bpm × duration_ms`. Real waveform / beat-grid data can be installed
/// afterwards with [`ActiveDJNode::set_response_waveforms`] /
/// [`ActiveDJNode::set_response_beat_grid`].
#[derive(Debug, Clone)]
pub struct TrackMeta {
    /// Track title (rendered as UTF-16 LE on the wire).
    pub title: String,
    /// Track artist (rendered as UTF-16 LE on the wire).
    pub artist: String,
    /// Track duration in milliseconds.
    pub duration_ms: u32,
    /// BPM (e.g. `128.0`). Stored as `BPM × 100` on the wire.
    pub bpm: f32,
    /// Track identifier (unique within the node).
    pub track_id: u32,
}

/// One CDJ-3000-style hot cue: a track position plus an RGB pad colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct HotCue {
    pub pos_ms: u32,
    pub color: [u8; 3],
}

/// Per-layer cue state owned by an [`ActiveDJNode`]. Slot 0 is the [CUE]
/// memory marker; slots 1..=8 are the A–H hot-cue pads.
#[derive(Debug, Clone, Copy)]
struct LayerCueState {
    cue_marker: Option<HotCue>,
    hot_cues: [Option<HotCue>; 8],
    loop_in_ms: u32,
    loop_out_ms: u32,
}

impl Default for LayerCueState {
    fn default() -> Self {
        Self {
            cue_marker: None,
            hot_cues: [None; 8],
            loop_in_ms: 0,
            loop_out_ms: 0,
        }
    }
}

impl LayerCueState {
    fn to_cue_data(self, layer_id: u8) -> CueData {
        let mut cues = [CueEntry::EMPTY; 18];
        if let Some(m) = self.cue_marker {
            cues[0] = CueEntry::new(1, m.pos_ms, m.pos_ms, m.color);
        }
        for (i, hc) in self.hot_cues.iter().enumerate() {
            if let Some(hc) = hc {
                cues[i + 1] = CueEntry::new(1, hc.pos_ms, hc.pos_ms, hc.color);
            }
        }
        CueData::build(layer_id, cues, self.loop_in_ms, self.loop_out_ms)
    }
}



// ---------------------------------------------------------------------------
// Internal shared state
// ---------------------------------------------------------------------------

/// Mutable per-layer / mixer state owned by an [`ActiveDJNode`].
///
/// Each layer and the mixer are published independently through
/// [`ArcSwap`] — readers (the periodic broadcast task, the dispatcher's
/// request handler, public snapshot accessors) load wait-free; writers
/// (public mutators, the same broadcast task) use the
/// `rcu`/`store` pattern to publish a new immutable snapshot atomically.
/// No `Mutex`, no `RwLock`.
struct ActiveNodeInner {
    layers: [ArcSwap<LayerSnapshot>; 8],
    cue_states: [ArcSwap<LayerCueState>; 8],
    mixer: ArcSwap<MixerSnapshot>,
}

impl Default for ActiveNodeInner {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| {
                let mut snap = LayerSnapshot::default();
                snap.bpm = Bpm((120.0 * 100.0) as u32);
                snap.speed = Speed::NORMAL;
                ArcSwap::from_pointee(snap)
            }),
            cue_states: std::array::from_fn(|_| {
                ArcSwap::from_pointee(LayerCueState::default())
            }),
            mixer: ArcSwap::from_pointee(MixerSnapshot::default()),
        }
    }
}

impl ActiveNodeInner {
    /// Take an owned snapshot of one layer.
    fn layer(&self, id: LayerId) -> Arc<LayerSnapshot> {
        self.layers[id.index()].load_full()
    }

    /// Read-copy-update a single layer: snapshot → modify → atomically
    /// republish. Retries on concurrent writes.
    fn update_layer<F>(&self, id: LayerId, mut f: F)
    where
        F: FnMut(&mut LayerSnapshot),
    {
        self.layers[id.index()].rcu(|cur| {
            let mut new = (**cur).clone();
            f(&mut new);
            Arc::new(new)
        });
    }

    /// Read-copy-update the mixer state.
    fn update_mixer_state<F>(&self, mut f: F)
    where
        F: FnMut(&mut MixerSnapshot),
    {
        self.mixer.rcu(|cur| {
            let mut new = (**cur).clone();
            f(&mut new);
            Arc::new(new)
        });
    }

    /// Read-copy-update the cue state for a layer.
    fn update_cue_state<F>(&self, id: LayerId, mut f: F)
    where
        F: FnMut(&mut LayerCueState),
    {
        self.cue_states[id.index()].rcu(|cur| {
            let mut new = (**cur).clone();
            f(&mut new);
            Arc::new(new)
        });
    }

    /// Snapshot every layer at once for packets that span all eight slots.
    fn layers_snapshot(&self) -> [Arc<LayerSnapshot>; 8] {
        std::array::from_fn(|i| self.layers[i].load_full())
    }

    fn build_time_packet(&self) -> Data {
        // Snapshot all eight layers up front — every field below reads from
        // the same atomic snapshot, so the packet is internally consistent
        // even if the layers mutate concurrently.
        let l = self.layers_snapshot();
        let tc = |snap: &LayerSnapshot| LayerTimecode {
            smpte_mode: smpte_to_u8(snap.smpte_mode),
            state: snap.tc_state,
            hours: snap.tc_hours,
            minutes: snap.tc_minutes,
            seconds: snap.tc_seconds,
            frames: snap.tc_frames,
        };
        Data::Time(TimePacketData {
            l1_time: l[0].current_time_ms,
            l2_time: l[1].current_time_ms,
            l3_time: l[2].current_time_ms,
            l4_time: l[3].current_time_ms,
            la_time: l[4].current_time_ms,
            lb_time: l[5].current_time_ms,
            lm_time: l[6].current_time_ms,
            lc_time: l[7].current_time_ms,
            l1_total_time: l[0].total_time_ms,
            l2_total_time: l[1].total_time_ms,
            l3_total_time: l[2].total_time_ms,
            l4_total_time: l[3].total_time_ms,
            la_total_time: l[4].total_time_ms,
            lb_total_time: l[5].total_time_ms,
            lm_total_time: l[6].total_time_ms,
            lc_total_time: l[7].total_time_ms,
            l1_beat_marker: l[0].beat_marker,
            l2_beat_marker: l[1].beat_marker,
            l3_beat_marker: l[2].beat_marker,
            l4_beat_marker: l[3].beat_marker,
            la_beat_marker: l[4].beat_marker,
            lb_beat_marker: l[5].beat_marker,
            lm_beat_marker: l[6].beat_marker,
            lc_beat_marker: l[7].beat_marker,
            l1_layer_state: l[0].state,
            l2_layer_state: l[1].state,
            l3_layer_state: l[2].state,
            l4_layer_state: l[3].state,
            la_layer_state: l[4].state,
            lb_layer_state: l[5].state,
            lm_layer_state: l[6].state,
            lc_layer_state: l[7].state,
            _reserved0: ReservedData::default(),
            smpte_mode: smpte_to_u8(l[0].smpte_mode),
            l1_timecode: tc(&l[0]),
            l2_timecode: tc(&l[1]),
            l3_timecode: tc(&l[2]),
            l4_timecode: tc(&l[3]),
            la_timecode: tc(&l[4]),
            lb_timecode: tc(&l[5]),
            lm_timecode: tc(&l[6]),
            lc_timecode: tc(&l[7]),
            l1_on_air: l[0].on_air,
            l2_on_air: l[1].on_air,
            l3_on_air: l[2].on_air,
            l4_on_air: l[3].on_air,
            la_on_air: l[4].on_air,
            lb_on_air: l[5].on_air,
            lm_on_air: l[6].on_air,
            lc_on_air: l[7].on_air,
        })
    }

    fn build_status_packet(&self) -> Data {
        let l = self.layers_snapshot();
        let name_bytes = |s: &str| -> [u8; 16] {
            let mut buf = [0u8; 16];
            let b = s.as_bytes();
            let n = b.len().min(16);
            buf[..n].copy_from_slice(&b[..n]);
            buf
        };
        Data::Status(StatusData {
            node_count: 1,
            node_listener_port: 65023,
            _reserved0: [0; 6],
            layer_1_source: l[0].source,
            layer_2_source: l[1].source,
            layer_3_source: l[2].source,
            layer_4_source: l[3].source,
            layer_a_source: l[4].source,
            layer_b_source: l[5].source,
            layer_m_source: l[6].source,
            layer_c_source: l[7].source,
            layer_1_status: LayerStatus::Variant,
            layer_2_status: LayerStatus::Variant,
            layer_3_status: LayerStatus::Variant,
            layer_4_status: LayerStatus::Variant,
            layer_a_status: LayerStatus::Variant,
            layer_b_status: LayerStatus::Variant,
            layer_m_status: LayerStatus::Variant,
            layer_c_status: LayerStatus::Variant,
            layer_1_track_id: l[0].track_id,
            layer_2_track_id: l[1].track_id,
            layer_3_track_id: l[2].track_id,
            layer_4_track_id: l[3].track_id,
            layer_a_track_id: l[4].track_id,
            layer_b_track_id: l[5].track_id,
            layer_m_track_id: l[6].track_id,
            layer_c_track_id: l[7].track_id,
            _reserved1: ReservedData::default(),
            smpte_mode: smpte_to_u8(l[0].smpte_mode),
            auto_master_mode: AutoMasterMode::Disabled,
            _reserved2: ReservedData::default(),
            app_specific: [0; 72],
            layer_1_name: name_bytes(&l[0].name),
            layer_2_name: name_bytes(&l[1].name),
            layer_3_name: name_bytes(&l[2].name),
            layer_4_name: name_bytes(&l[3].name),
            layer_a_name: name_bytes(&l[4].name),
            layer_b_name: name_bytes(&l[5].name),
            layer_m_name: name_bytes(&l[6].name),
            layer_c_name: name_bytes(&l[7].name),
        })
    }

    fn build_metrics_packet(&self, layer: LayerId) -> Data {
        let snap = self.layer(layer);
        Data::Metrics(MetricsData {
            data_type: 2,
            layer_id: layer.as_packet_id(),
            _reserved0: ReservedData::default(),
            layer_state: snap.state,
            _reserved1: ReservedData::default(),
            sync_master: snap.sync_master as u8,
            _reserved2: ReservedData::default(),
            beat_marker: snap.beat_marker,
            track_length: snap.track_length_ms,
            current_position: snap.position_ms,
            speed: snap.speed.0,
            _reserved3: ReservedData::default(),
            beat_number: snap.beat_number,
            _reserved4: ReservedData::default(),
            bpm: snap.bpm.0,
            pitch_bend: snap.pitch_bend,
            track_id: snap.track_id,
        })
    }

    fn build_meta_packet(&self, layer: LayerId) -> Data {
        let snap = self.layer(layer);
        let mut artist_buf = [0u8; 256];
        let mut title_buf = [0u8; 256];
        encode_utf16le(&snap.artist, &mut artist_buf);
        encode_utf16le(&snap.title, &mut title_buf);
        Data::Meta(MetaData {
            data_type: 4,
            layer_id: layer.as_packet_id(),
            _reserved0: ReservedData::default(),
            _reserved1: ReservedData::default(),
            track_artist: artist_buf,
            track_title: title_buf,
            track_key: snap.track_key,
            track_id: snap.track_id,
        })
    }

    fn build_mixer_packet(&self) -> Data {
        let s = self.mixer.load_full();
        let channels: [MixerChannel; 6] = std::array::from_fn(|i| {
            let ch = &s.channels[i];
            MixerChannel {
                source_select: ch.source_select,
                audio_level: ch.audio_level,
                fader_level: ch.fader_level,
                trim_level: ch.trim_level,
                comp_level: ch.comp_level,
                eq_hi_level: ch.eq_hi,
                eq_hi_mid_level: ch.eq_hi_mid,
                eq_low_mid_level: ch.eq_low_mid,
                eq_low_level: ch.eq_low,
                filter_color: ch.filter_color,
                send: ch.send,
                cue_a: ch.cue_a as u8,
                cue_b: ch.cue_b as u8,
                crossfader_assign: ch.crossfader_assign,
                _reserved: [0; 10],
            }
        });
        let mut mixer_name = [0u8; 16];
        let nb = s.mixer_name.as_bytes();
        let n = nb.len().min(16);
        mixer_name[..n].copy_from_slice(&nb[..n]);
        Data::Mixer(MixerData {
            data_type: 150,
            mixer_id: s.mixer_id,
            mixer_type: s.mixer_type,
            _reserved0: ReservedData::default(),
            _reserved1: ReservedData::default(),
            mixer_name,
            _reserved2: ReservedData::default(),
            _reserved3: ReservedData::default(),
            mic_eq_hi: s.mic_eq_hi,
            mic_eq_low: s.mic_eq_low,
            master_audio_level: s.master_audio_level,
            master_fader_level: s.master_fader_level,
            _reserved4: ReservedData::default(),
            link_cue_a: s.link_cue_a as u8,
            link_cue_b: s.link_cue_b as u8,
            master_filter: s.master_filter,
            _reserved5: ReservedData::default(),
            master_cue_a: s.master_cue_a as u8,
            master_cue_b: s.master_cue_b as u8,
            _reserved6: ReservedData::default(),
            master_isolator_on_off: s.isolator_on as u8,
            master_isolator_hi: s.isolator_hi,
            master_isolator_mid: s.isolator_mid,
            master_isolator_low: s.isolator_low,
            _reserved7: ReservedData::default(),
            filter_hpf: s.filter_hpf,
            filter_lpf: s.filter_lpf,
            filter_resonance: s.filter_resonance,
            _reserved8: ReservedData::default(),
            send_fx_effect: s.send_fx_effect,
            send_fx_ext_1: 0,
            send_fx_ext_2: 0,
            send_fx_master_mix: s.send_fx_master_mix as u8,
            send_fx_size_feedback: s.send_fx_size_feedback,
            send_fx_time: s.send_fx_time,
            send_fx_hpf: s.send_fx_hpf,
            send_fx_level: s.send_fx_level,
            send_return_3_source_select: 0,
            send_return_3_type: 0,
            send_return_3_on_off: 0,
            send_return_3_level: 0,
            _reserved9: ReservedData::default(),
            channel_fader_curve: s.channel_fader_curve,
            cross_fader_curve: s.crossfader_curve,
            cross_fader: s.crossfader,
            beat_fx_on_off: s.beat_fx_on as u8,
            beat_fx_level_depth: s.beat_fx_level_depth,
            beat_fx_channel_select: s.beat_fx_channel_select,
            beat_fx_select: s.beat_fx_select,
            beat_fx_freq_hi: 0,
            beat_fx_freq_mid: 0,
            beat_fx_freq_low: 0,
            headphones_pre_eq: s.headphones_pre_eq as u8,
            headphones_a_level: s.headphones_a_level,
            headphones_a_mix: s.headphones_a_mix,
            headphones_b_level: s.headphones_b_level,
            headphones_b_mix: s.headphones_b_mix,
            booth_level: s.booth_level,
            booth_eq_hi: s.booth_eq_hi,
            booth_eq_low: s.booth_eq_low,
            _reserved10: [0; 10],
            channels,
        })
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Broadcasts this process's state as a TCNet DJ controller node.
///
/// Created via [`TCNetClient::create_active_node`](crate::TCNetClient::create_active_node).
/// A background tokio task drives the periodic broadcasts:
///
/// * **Time packets** (message type 254) every 20 ms — the high-frequency
///   clock everyone syncs to.
/// * **Status packets** (message type 5) every 1 s — the directory of layer
///   sources / loaded tracks / layer names.
/// * **Metrics packets** (message type 200/2) every 50 ms for every layer
///   whose state is `Playing` or `Looping`.
///
/// In addition, the node caches pre-built **response packets** for waveform,
/// beat-grid, cue, artwork and mixer queries — peers that issue a
/// [`RequestData`](crate::protocol::RequestData) get these answered
/// automatically. Replace the placeholder waveforms/beat-grid with real
/// audio-derived data via [`set_response_waveforms`](Self::set_response_waveforms)
/// and [`set_response_beat_grid`](Self::set_response_beat_grid).
pub struct ActiveDJNode {
    inner: Arc<ActiveNodeInner>,
    broadcast_tx: kanal::Sender<Data>,
    slave_unicast_tx: kanal::Sender<Data>,
    #[allow(dead_code)]
    time_tx: kanal::Sender<Data>,
    pub(crate) response_data: SharedResponseData,
}

impl ActiveDJNode {
    pub(crate) fn new(
        broadcast_tx: kanal::Sender<Data>,
        slave_unicast_tx: kanal::Sender<Data>,
        time_tx: kanal::Sender<Data>,
        response_data: SharedResponseData,
        runtime: &Runtime,
    ) -> Self {
        let inner = Arc::new(ActiveNodeInner::default());
        let inner_bg = inner.clone();
        let bcast = broadcast_tx.clone();
        let sucast = slave_unicast_tx.clone();
        let tcast = time_tx.clone();
        let rd_bg = response_data.clone();

        runtime.spawn(async move {
            let mut time_tick = interval(Duration::from_millis(20));
            let mut status_tick = interval(Duration::from_secs(1));
            let mut metrics_tick = interval(Duration::from_millis(50));
            loop {
                tokio::select! {
                    _ = time_tick.tick() => {
                        let data = inner_bg.build_time_packet();
                        let _ = tcast.try_send(data);
                    }
                    _ = status_tick.tick() => {
                        let status = inner_bg.build_status_packet();
                        let _ = bcast.try_send(status);
                        // Re-unicast a Meta packet for every layer that
                        // currently holds a track. Meta is otherwise
                        // only sent once at `load_track`, so a slave
                        // node that joined *after* the load never
                        // learns the artist / title (only the 16-byte
                        // `layer_N_name` from Status broadcasts). The
                        // 1 Hz cadence is cheap and lets late-joining
                        // slaves catch up within a second.
                        for &id in LayerId::ALL.iter() {
                            if inner_bg.layer(id).track_id != 0 {
                                let meta = inner_bg.build_meta_packet(id);
                                rd_bg.layers[id.index()]
                                    .last_meta
                                    .store(std::sync::Arc::new(Some(meta.clone())));
                                let _ = sucast.try_send(meta);
                            }
                        }
                    }
                    _ = metrics_tick.tick() => {
                        // Send Metrics for every layer that carries a track,
                        // not just playing ones. Real CDJs broadcast their
                        // current state (track length, BPM, sync master)
                        // even when stopped/paused/cued — otherwise a
                        // slave that joined *after* the load (or after the
                        // last state transition) would never learn the
                        // track's length or BPM, because `MetaData` only
                        // carries title/artist. Without this every
                        // not-currently-playing deck reads as
                        // `track_length_ms=0, bpm=0` on the slave, which
                        // breaks every downstream consumer (analysis,
                        // playhead, BPM-derived bar grid, …).
                        for &id in LayerId::ALL.iter() {
                            if inner_bg.layer(id).track_id != 0 {
                                let data = inner_bg.build_metrics_packet(id);
                                rd_bg.layers[id.index()]
                                    .last_metrics
                                    .store(std::sync::Arc::new(Some(data.clone())));
                                let _ = sucast.try_send(data);
                            }
                        }
                    }
                }
            }
        });

        Self {
            inner,
            broadcast_tx,
            slave_unicast_tx,
            time_tx,
            response_data,
        }
    }

    fn send_broadcast(&self, data: Data) -> Result<(), SendError> {
        self.broadcast_tx.try_send(data).map(|_| ())
    }

    fn send_slave_unicast(&self, data: Data) -> Result<(), SendError> {
        self.slave_unicast_tx.try_send(data).map(|_| ())
    }

    fn update_metrics(&self, layer: LayerId) -> Result<(), SendError> {
        let data = self.inner.build_metrics_packet(layer);
        self.response_data.layers[layer.index()]
            .last_metrics
            .store(std::sync::Arc::new(Some(data.clone())));
        self.send_slave_unicast(data)
    }

    fn update_meta(&self, layer: LayerId) -> Result<(), SendError> {
        let data = self.inner.build_meta_packet(layer);
        self.response_data.layers[layer.index()]
            .last_meta
            .store(std::sync::Arc::new(Some(data.clone())));
        self.send_slave_unicast(data)
    }

    fn update_mixer(&self) -> Result<(), SendError> {
        let data = self.inner.build_mixer_packet();
        self.response_data
            .last_mixer
            .store(std::sync::Arc::new(Some(data.clone())));
        self.send_slave_unicast(data)
    }

    // --- playback ---

    /// Mark `layer` as `Playing` and broadcast an updated [`MetricsData`].
    pub fn play(&mut self, layer: LayerId) -> Result<(), SendError> {
        self.inner
            .update_layer(layer, |snap| snap.state = LayerState::Playing);
        self.update_metrics(layer)
    }

    /// Mark `layer` as `Paused` at its current position and broadcast an
    /// updated [`MetricsData`].
    pub fn pause(&mut self, layer: LayerId) -> Result<(), SendError> {
        self.inner
            .update_layer(layer, |snap| snap.state = LayerState::Paused);
        self.update_metrics(layer)
    }

    /// Mark `layer` as `Stopped`, snap the playhead to position 0 and
    /// broadcast an updated [`MetricsData`].
    pub fn stop(&mut self, layer: LayerId) -> Result<(), SendError> {
        self.inner.update_layer(layer, |l| {
            l.state = LayerState::Stopped;
            l.position_ms = 0;
            l.current_time_ms = 0;
        });
        self.update_metrics(layer)
    }

    /// Move `layer` to `pos_ms` and report it as `CueButtonDown` — equivalent
    /// to holding a hot-cue on a physical CDJ.
    pub fn cue(&mut self, layer: LayerId, pos_ms: u32) -> Result<(), SendError> {
        self.inner.update_layer(layer, |l| {
            l.state = LayerState::CueButtonDown;
            l.position_ms = pos_ms;
            l.current_time_ms = pos_ms;
        });
        self.update_metrics(layer)
    }

    // --- track loading ---

    /// Load a new track on `layer`.
    ///
    /// Populates the layer state from `info`, marks it `Stopped` at position 0,
    /// pre-builds the response packets that peers can later request:
    ///
    /// * [`SmallWaveformData`] / [`BigWaveformData`] — placeholder uniform-blue
    ///   waveform; call [`set_response_waveforms`](Self::set_response_waveforms)
    ///   to overwrite with audio-derived data.
    /// * [`BeatGridHeader`] — synthesised from `info.bpm × info.duration_ms`
    ///   (downbeat every 4 beats); call
    ///   [`set_response_beat_grid`](Self::set_response_beat_grid) to overwrite
    ///   with real detection results.
    /// * [`CueData`] — empty.
    /// * [`ArtworkFileData`] — empty.
    ///
    /// Finally broadcasts a [`StatusData`] packet and unicasts a [`MetaData`]
    /// packet so peers see the new track immediately.
    pub fn load_track(&mut self, layer: LayerId, info: TrackMeta) -> Result<(), SendError> {
        // Guard against pathological BPM values. The beat-grid synthesiser
        // below loops while `(beat * 60000.0 / bpm) as u32 <= duration_ms`;
        // a non-finite or non-positive bpm makes that condition forever
        // false and the loop overflows `u16` (debug) or spins (release).
        if !info.bpm.is_finite() || info.bpm <= 0.0 {
            return Err(SendError::Closed);
        }
        self.inner.update_layer(layer, |l| {
            l.state = LayerState::Stopped;
            l.track_id = info.track_id;
            l.track_length_ms = info.duration_ms;
            l.total_time_ms = info.duration_ms;
            l.position_ms = 0;
            l.current_time_ms = 0;
            l.bpm = Bpm((info.bpm * 100.0) as u32);
            l.artist = info.artist.clone();
            l.title = info.title.clone();
            l.name = info.title.clone();
        });
        // Clear per-track cue state.
        self.inner
            .update_cue_state(layer, |cs| *cs = LayerCueState::default());

        // Pre-build response packets so the dispatcher can answer REQUEST packets.
        // Each field is its own ArcSwap — wait-free atomic stores, no locks.
        {
            let ld = &self.response_data.layers[layer.index()];
            let lid = layer.as_packet_id();

            // SmallWaveform: 2400 bytes, uniform amplitude 0x40 (blue).
            let mut waveform_bytes = [0u8; 2400];
            for chunk in waveform_bytes.chunks_mut(2) {
                chunk[0] = 0x40; // amplitude
                chunk[1] = 0x03; // blue color
            }
            ld.small_waveform_packet.store(std::sync::Arc::new(Some(
                Data::SmallWaveform(SmallWaveformData::new(lid, waveform_bytes)),
            )));

            // BigWaveform: same bytes, split into 4400-byte clusters.
            let big_raw = waveform_bytes.to_vec();
            let total = big_raw.len() as u32;
            let big_packets: Vec<Data> = big_raw
                .chunks(4400)
                .enumerate()
                .map(|(i, chunk)| {
                    let n = (total as usize).div_ceil(4400).max(1) as u32;
                    Data::BigWaveform(BigWaveformData::new_packet(
                        lid,
                        total,
                        n,
                        i as u32,
                        chunk.to_vec(),
                    ))
                })
                .collect();
            ld.big_waveform_packets
                .store(std::sync::Arc::new(big_packets));

            // BeatGrid: compute entries from BPM and duration.
            let beat_grid_packets: Vec<Data> = {
                use deku::DekuContainerWrite;
                let beat_interval_ms = 60_000.0 / info.bpm;
                let mut entries: Vec<BeatGridEntry> = Vec::new();
                let mut beat = 0u32;
                loop {
                    let ts = (beat as f32 * beat_interval_ms) as u32;
                    if ts > info.duration_ms {
                        break;
                    }
                    let beat_type = if beat.is_multiple_of(4) { 20u8 } else { 10u8 };
                    entries.push(BeatGridEntry::new(beat as u16 + 1, beat_type, ts));
                    beat += 1;
                }
                let raw: Vec<u8> = entries
                    .iter()
                    .flat_map(|e| e.to_bytes().unwrap_or_default())
                    .collect();
                let total_bg = raw.len() as u32;
                let n = raw.chunks(2400).count().max(1) as u32;
                if raw.is_empty() {
                    vec![Data::BeatGrid(BeatGridHeader::new_packet(
                        lid,
                        0,
                        1,
                        0,
                        vec![],
                    ))]
                } else {
                    raw.chunks(2400)
                        .enumerate()
                        .map(|(i, chunk)| {
                            Data::BeatGrid(BeatGridHeader::new_packet(
                                lid,
                                total_bg,
                                n,
                                i as u32,
                                chunk.to_vec(),
                            ))
                        })
                        .collect()
                }
            };
            ld.beat_grid_packets
                .store(std::sync::Arc::new(beat_grid_packets));

            // Cue: clear all hot cues + the [CUE] memory marker on track load.
            ld.cue_packet.store(std::sync::Arc::new(Some(Data::Cue(
                CueData::build(lid, [CueEntry::EMPTY; 18], 0, 0),
            ))));

            // Artwork: empty placeholder.
            ld.artwork_packets
                .store(std::sync::Arc::new(vec![Data::ArtworkFile(
                    ArtworkFileData::new_packet(lid, 0, 1, 0, vec![]),
                )]));
        }

        // Populate last_metrics so stopped tracks can respond to MetricsData requests.
        let _ = self.update_metrics(layer);

        let status = self.inner.build_status_packet();
        self.send_broadcast(status)?;
        self.update_meta(layer)
    }

    /// Clear `layer` back to the default empty state and broadcast a
    /// [`StatusData`] so peers stop showing the previous track.
    pub fn unload_track(&mut self, layer: LayerId) -> Result<(), SendError> {
        self.inner
            .update_layer(layer, |snap| *snap = LayerSnapshot::default());
        let data = self.inner.build_status_packet();
        self.send_broadcast(data)
    }

    // --- position / tempo ---

    /// Move `layer`'s playhead to `ms`. Broadcasts an updated [`MetricsData`].
    pub fn set_position_ms(&mut self, layer: LayerId, ms: u32) -> Result<(), SendError> {
        self.inner.update_layer(layer, |l| {
            l.position_ms = ms;
            l.current_time_ms = ms;
        });
        self.update_metrics(layer)
    }

    /// Atomically update playhead position, bar-phase marker, and absolute
    /// beat number for `layer`, broadcasting a single [`MetricsData`].
    ///
    /// `beat_marker` is the 0–255 position within a 4-beat bar (TCNet wire
    /// convention; consumers map it to a beat-1-of-4 indicator with
    /// `marker * 4 / 256`). `beat_number` is the 1-based absolute beat
    /// counter from track start.
    ///
    /// Use this instead of [`set_position_ms`](Self::set_position_ms) when
    /// the broadcaster knows the beat-grid math; emits one packet instead of
    /// three.
    pub fn set_layer_position(
        &mut self,
        layer: LayerId,
        pos_ms: u32,
        beat_marker: u8,
        beat_number: u32,
    ) -> Result<(), SendError> {
        self.inner.update_layer(layer, |l| {
            l.position_ms = pos_ms;
            l.current_time_ms = pos_ms;
            l.beat_marker = beat_marker;
            l.beat_number = beat_number;
        });
        self.update_metrics(layer)
    }

    /// Set the layer's BPM (encoded as `bpm × 100` on the wire).
    pub fn set_bpm(&mut self, layer: LayerId, bpm: f32) -> Result<(), SendError> {
        self.inner
            .update_layer(layer, |l| l.bpm = Bpm((bpm * 100.0) as u32));
        self.update_metrics(layer)
    }

    /// Set the layer's playback [`Speed`] (32768 = 100%).
    pub fn set_speed(&mut self, layer: LayerId, speed: Speed) -> Result<(), SendError> {
        self.inner.update_layer(layer, |l| l.speed = speed);
        self.update_metrics(layer)
    }

    /// Mark `layer` as the sync master (or clear the flag).
    pub fn set_sync_master(&mut self, layer: LayerId, master: bool) -> Result<(), SendError> {
        self.inner
            .update_layer(layer, |l| l.sync_master = master);
        self.update_metrics(layer)
    }

    // --- cue / hot cues / loop ---

    /// Update the cached cue-data response for `layer` and broadcast nothing
    /// (cue data is request/response — peers fetch it on demand).
    fn rebuild_cue_packet(&self, layer: LayerId) {
        let lid = layer.as_packet_id();
        let cue_data = self.inner.cue_states[layer.index()]
            .load()
            .to_cue_data(lid);
        self.response_data.layers[layer.index()]
            .cue_packet
            .store(std::sync::Arc::new(Some(Data::Cue(cue_data))));
    }

    /// Set the persistent [CUE] memory marker for `layer`. Peers that issue a
    /// [`RequestData`](crate::protocol::RequestData) for
    /// [`CueData`](crate::RequestDataType::CueData) will receive the updated
    /// cue point in slot 0 of the [`CueData`] response.
    ///
    /// Pass `None` to clear the marker. The slot-0 colour is fixed to CDJ
    /// orange (RGB `255, 128, 0`).
    pub fn set_cue_marker(&mut self, layer: LayerId, pos_ms: Option<u32>) {
        self.inner.update_cue_state(layer, |cs| {
            cs.cue_marker = pos_ms.map(|p| HotCue {
                pos_ms: p,
                color: [255, 128, 0],
            });
        });
        self.rebuild_cue_packet(layer);
    }

    /// Replace the hot-cue table for `layer` (slots A–H, indexed 0..8). `None`
    /// entries clear the corresponding pad. Updates the cached
    /// [`CueData`](crate::protocol::CueData) response.
    pub fn set_hot_cues(&mut self, layer: LayerId, hot_cues: [Option<HotCue>; 8]) {
        self.inner
            .update_cue_state(layer, |cs| cs.hot_cues = hot_cues);
        self.rebuild_cue_packet(layer);
    }

    /// Set the layer's active loop range in milliseconds (or `(0, 0)` to
    /// clear).
    pub fn set_loop_range(&mut self, layer: LayerId, in_ms: u32, out_ms: u32) {
        self.inner.update_cue_state(layer, |cs| {
            cs.loop_in_ms = in_ms;
            cs.loop_out_ms = out_ms;
        });
        self.rebuild_cue_packet(layer);
    }

    // --- mixer ---

    /// Set the master fader level (0–255). Broadcasts an updated [`MixerData`].
    pub fn set_master_fader(&mut self, level: u8) -> Result<(), SendError> {
        self.inner
            .update_mixer_state(|m| m.master_fader_level = level);
        self.update_mixer()
    }

    /// Set the crossfader position (0 = full A, 255 = full B).
    pub fn set_crossfader(&mut self, pos: u8) -> Result<(), SendError> {
        self.inner.update_mixer_state(|m| m.crossfader = pos);
        self.update_mixer()
    }

    /// Set channel `ch`'s fader level (0–255). Channels are 0-based, 6 total.
    pub fn set_channel_fader(&mut self, ch: usize, level: u8) -> Result<(), SendError> {
        if ch < 6 {
            self.inner
                .update_mixer_state(|m| m.channels[ch].fader_level = level);
        }
        self.update_mixer()
    }

    /// Set channel `ch`'s trim / gain (0–255).
    pub fn set_channel_trim(&mut self, ch: usize, level: u8) -> Result<(), SendError> {
        if ch < 6 {
            self.inner
                .update_mixer_state(|m| m.channels[ch].trim_level = level);
        }
        self.update_mixer()
    }

    /// Set channel `ch`'s three-band EQ (each 0–255).
    pub fn set_channel_eq(&mut self, ch: usize, hi: u8, mid: u8, low: u8) -> Result<(), SendError> {
        if ch < 6 {
            self.inner.update_mixer_state(|m| {
                m.channels[ch].eq_hi = hi;
                m.channels[ch].eq_hi_mid = mid;
                m.channels[ch].eq_low = low;
            });
        }
        self.update_mixer()
    }

    /// Set channel `ch`'s colour filter / sound-colour FX position (0–255).
    pub fn set_channel_filter(&mut self, ch: usize, val: u8) -> Result<(), SendError> {
        if ch < 6 {
            self.inner
                .update_mixer_state(|m| m.channels[ch].filter_color = val);
        }
        self.update_mixer()
    }

    /// Toggle channel `ch`'s assignment to the headphone CUE A / B buses.
    pub fn set_channel_cue(
        &mut self,
        ch: usize,
        cue_a: bool,
        cue_b: bool,
    ) -> Result<(), SendError> {
        if ch < 6 {
            self.inner.update_mixer_state(|m| {
                m.channels[ch].cue_a = cue_a;
                m.channels[ch].cue_b = cue_b;
            });
        }
        self.update_mixer()
    }

    /// Set the layer's on-air state — the fader-position byte (0 = down,
    /// 255 = fully up) carried in [`TimePacketData`]'s `l*_on_air` fields.
    ///
    /// Does not broadcast immediately; the change is picked up by the next
    /// scheduled Time-packet emission (~20 ms cadence).
    pub fn set_on_air(&mut self, layer: LayerId, on_air: u8) {
        self.inner.update_layer(layer, |l| l.on_air = on_air);
    }

    /// Overwrite the cached beat-grid response for `layer` with real
    /// audio-derived entries.
    ///
    /// `entries` is a sequence of `(beat_number, beat_type, timestamp_ms)`
    /// tuples following the TCNet convention (`beat_type = 20` for downbeats,
    /// `10` for upbeats). The payload is automatically split across the
    /// spec-mandated 2400-byte clusters into one or more [`BeatGridHeader`]
    /// chunks. Peers that issue a
    /// [`RequestData`](crate::protocol::RequestData) for
    /// [`BeatGridData`](crate::RequestDataType::BeatGridData) will receive the
    /// new grid on the next exchange.
    pub fn set_response_beat_grid(&self, layer: LayerId, entries: &[(u16, u8, u32)]) {
        use deku::DekuContainerWrite;
        let lid = layer.as_packet_id();
        let serialized: Vec<u8> = entries
            .iter()
            .flat_map(|(beat_number, beat_type, beat_timestamp)| {
                BeatGridEntry::new(*beat_number, *beat_type, *beat_timestamp)
                    .to_bytes()
                    .unwrap_or_default()
            })
            .collect();
        let total = serialized.len() as u32;

        let beat_grid_packets: Vec<Data> = if serialized.is_empty() {
            vec![Data::BeatGrid(BeatGridHeader::new_packet(
                lid,
                0,
                1,
                0,
                vec![],
            ))]
        } else {
            let n_packets = (serialized.len()).div_ceil(2400).max(1) as u32;
            serialized
                .chunks(2400)
                .enumerate()
                .map(|(i, chunk)| {
                    Data::BeatGrid(BeatGridHeader::new_packet(
                        lid,
                        total,
                        n_packets,
                        i as u32,
                        chunk.to_vec(),
                    ))
                })
                .collect()
        };
        self.response_data.layers[layer.index()]
            .beat_grid_packets
            .store(std::sync::Arc::new(beat_grid_packets));
    }

    /// Overwrite the cached small + big waveform responses for `layer`.
    ///
    /// * `small` — exactly 2400 bytes (1200 `(level, colour)` pairs); see
    ///   [`SmallWaveformData`] for the byte-pair layout.
    /// * `big` — variable-length payload, auto-split into 4400-byte chunks.
    ///
    /// Call after [`load_track`](Self::load_track) to replace the placeholder
    /// bytes with audio-derived data. Peers' subsequent
    /// [`SmallWaveformData`](crate::RequestDataType::SmallWaveformData) /
    /// [`LargeWaveformData`](crate::RequestDataType::LargeWaveformData)
    /// requests see the new bytes.
    pub fn set_response_waveforms(&self, layer: LayerId, small: [u8; 2400], big: Vec<u8>) {
        let ld = &self.response_data.layers[layer.index()];
        let lid = layer.as_packet_id();
        ld.small_waveform_packet
            .store(std::sync::Arc::new(Some(Data::SmallWaveform(
                SmallWaveformData::new(lid, small),
            ))));

        let total = big.len() as u32;
        // Split big into 4400-byte clusters (matches the existing placeholder convention).
        let big_packets: Vec<Data> = if big.is_empty() {
            Vec::new()
        } else {
            let n_packets = (total as usize).div_ceil(4400).max(1) as u32;
            big.chunks(4400)
                .enumerate()
                .map(|(i, chunk)| {
                    Data::BigWaveform(BigWaveformData::new_packet(
                        lid,
                        total,
                        n_packets,
                        i as u32,
                        chunk.to_vec(),
                    ))
                })
                .collect()
        };
        ld.big_waveform_packets
            .store(std::sync::Arc::new(big_packets));
    }

    // --- state read access ---

    /// Snapshot of the eight layer states currently being broadcast.
    pub fn layers(&self) -> Vec<LayerSnapshot> {
        self.inner
            .layers_snapshot()
            .iter()
            .map(|arc| (**arc).clone())
            .collect()
    }

    /// Snapshot of the mixer state currently being broadcast.
    pub fn mixer(&self) -> MixerSnapshot {
        (**self.inner.mixer.load()).clone()
    }

    /// Snapshot of one channel's state (clamped to channel 5 if `ch >= 6`).
    pub fn channel_snapshot(&self, ch: usize) -> ChannelSnapshot {
        self.inner.mixer.load().channels[ch.min(5)].clone()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn smpte_to_u8(mode: SmpteMode) -> u8 {
    match mode {
        SmpteMode::Fps24 => 24,
        SmpteMode::Fps25 => 25,
        SmpteMode::Fps2997 => 29,
        SmpteMode::Fps30 => 30,
    }
}

fn encode_utf16le(s: &str, buf: &mut [u8]) {
    let mut i = 0;
    for ch in s.encode_utf16() {
        if i + 2 > buf.len() {
            break;
        }
        buf[i] = (ch & 0xff) as u8;
        buf[i + 1] = (ch >> 8) as u8;
        i += 2;
    }
}
