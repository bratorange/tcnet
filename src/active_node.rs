use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::interval;
use crate::node::dj_controller::{ChannelSnapshot, LayerSnapshot, MixerSnapshot};
use crate::node::tcnet_packet::Data;
use crate::node::tcnet_packet_serde::{
    ArtworkFileData, BeatGridEntry, BeatGridHeader, BigWaveformData, CueData, SmallWaveformData,
    AutoMasterMode, Bpm, LayerId, LayerState, LayerStatus, LayerTimecode,
    MetaData, MetricsData, MixerChannel, MixerData, ReservedData, SmpteMode,
    Speed, StatusData, TimePacketData,
};
use crate::node::response_data::SharedResponseData;

pub use kanal::SendError;

// ---------------------------------------------------------------------------
// Public metadata type for track loading
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub duration_ms: u32,
    pub bpm: f32,
    pub track_id: u32,
}

// ---------------------------------------------------------------------------
// Internal shared state
// ---------------------------------------------------------------------------

struct ActiveNodeInner {
    layers: Vec<LayerSnapshot>,
    mixer: MixerSnapshot,
}

impl Default for ActiveNodeInner {
    fn default() -> Self {
        let mut layers: Vec<LayerSnapshot> = LayerId::ALL.iter()
            .map(|_| LayerSnapshot::default())
            .collect();
        for l in &mut layers {
            l.bpm = Bpm((120.0 * 100.0) as u32);
            l.speed = Speed::NORMAL;
        }
        Self { layers, mixer: MixerSnapshot::default() }
    }
}

impl ActiveNodeInner {
    fn layer(&self, id: LayerId) -> &LayerSnapshot { &self.layers[id.index()] }
    fn layer_mut(&mut self, id: LayerId) -> &mut LayerSnapshot { &mut self.layers[id.index()] }

    fn build_time_packet(&self) -> Data {
        let l = &self.layers;
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
        let l = &self.layers;
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
            auto_master_mode: AutoMasterMode::Variant,
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
        let s = &self.mixer;
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

pub struct ActiveDJNode {
    inner: Arc<Mutex<ActiveNodeInner>>,
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
        let inner = Arc::new(Mutex::new(ActiveNodeInner::default()));
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
                        let data = inner_bg.lock().unwrap().build_time_packet();
                        let _ = tcast.try_send(data);
                    }
                    _ = status_tick.tick() => {
                        let data = inner_bg.lock().unwrap().build_status_packet();
                        let _ = bcast.try_send(data);
                    }
                    _ = metrics_tick.tick() => {
                        let inner = inner_bg.lock().unwrap();
                        for &id in LayerId::ALL.iter() {
                            if inner.layer(id).state.is_playing() {
                                let data = inner.build_metrics_packet(id);
                                if let Ok(mut rd) = rd_bg.lock() {
                                    rd.layers[id.index()].last_metrics = Some(data.clone());
                                }
                                let _ = sucast.try_send(data);
                            }
                        }
                    }
                }
            }
        });

        Self { inner, broadcast_tx, slave_unicast_tx, time_tx, response_data }
    }

    fn send_broadcast(&self, data: Data) -> Result<(), SendError> {
        self.broadcast_tx.try_send(data).map(|_| ())
    }

    fn send_slave_unicast(&self, data: Data) -> Result<(), SendError> {
        self.slave_unicast_tx.try_send(data).map(|_| ())
    }

    fn update_metrics(&self, layer: LayerId) -> Result<(), SendError> {
        let data = self.inner.lock().unwrap().build_metrics_packet(layer);
        if let Ok(mut rd) = self.response_data.lock() {
            rd.layers[layer.index()].last_metrics = Some(data.clone());
        }
        self.send_slave_unicast(data)
    }

    fn update_meta(&self, layer: LayerId) -> Result<(), SendError> {
        let data = self.inner.lock().unwrap().build_meta_packet(layer);
        if let Ok(mut rd) = self.response_data.lock() {
            rd.layers[layer.index()].last_meta = Some(data.clone());
        }
        self.send_slave_unicast(data)
    }

    fn update_mixer(&self) -> Result<(), SendError> {
        let data = self.inner.lock().unwrap().build_mixer_packet();
        if let Ok(mut rd) = self.response_data.lock() {
            rd.last_mixer = Some(data.clone());
        }
        self.send_slave_unicast(data)
    }

    // --- playback ---

    pub fn play(&mut self, layer: LayerId) -> Result<(), SendError> {
        self.inner.lock().unwrap().layer_mut(layer).state = LayerState::Playing;
        self.update_metrics(layer)
    }

    pub fn pause(&mut self, layer: LayerId) -> Result<(), SendError> {
        self.inner.lock().unwrap().layer_mut(layer).state = LayerState::Paused;
        self.update_metrics(layer)
    }

    pub fn stop(&mut self, layer: LayerId) -> Result<(), SendError> {
        {
            let mut inner = self.inner.lock().unwrap();
            let l = inner.layer_mut(layer);
            l.state = LayerState::Stopped;
            l.position_ms = 0;
            l.current_time_ms = 0;
        }
        self.update_metrics(layer)
    }

    pub fn cue(&mut self, layer: LayerId, pos_ms: u32) -> Result<(), SendError> {
        {
            let mut inner = self.inner.lock().unwrap();
            let l = inner.layer_mut(layer);
            l.state = LayerState::CueButtonDown;
            l.position_ms = pos_ms;
            l.current_time_ms = pos_ms;
        }
        self.update_metrics(layer)
    }

    // --- track loading ---

    pub fn load_track(&mut self, layer: LayerId, info: TrackMeta) -> Result<(), SendError> {
        {
            let mut inner = self.inner.lock().unwrap();
            let l = inner.layer_mut(layer);
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
        }

        // Pre-build response packets so the dispatcher can answer REQUEST packets.
        if let Ok(mut rd) = self.response_data.lock() {
            let ld = &mut rd.layers[layer.index()];
            let lid = layer.as_packet_id();

            // SmallWaveform: 2400 bytes, uniform amplitude 0x40 (blue).
            let mut waveform_bytes = [0u8; 2400];
            for chunk in waveform_bytes.chunks_mut(2) {
                chunk[0] = 0x40; // amplitude
                chunk[1] = 0x03; // blue color
            }
            ld.small_waveform_packet = Some(Data::SmallWaveform(SmallWaveformData::new(lid, waveform_bytes)));

            // BigWaveform: same bytes, split into 4400-byte clusters.
            let big_raw = waveform_bytes.to_vec();
            let total = big_raw.len() as u32;
            ld.big_waveform_packets = big_raw.chunks(4400)
                .enumerate()
                .map(|(i, chunk)| {
                    let n = (total as usize).div_ceil(4400).max(1) as u32;
                    Data::BigWaveform(BigWaveformData::new_packet(lid, total, n, i as u32, chunk.to_vec()))
                })
                .collect();

            // BeatGrid: compute entries from BPM and duration.
            {
                use deku::DekuContainerWrite;
                let beat_interval_ms = 60_000.0 / info.bpm;
                let mut entries: Vec<BeatGridEntry> = Vec::new();
                let mut beat = 0u32;
                loop {
                    let ts = (beat as f32 * beat_interval_ms) as u32;
                    if ts > info.duration_ms { break; }
                    let beat_type = if beat % 4 == 0 { 20u8 } else { 10u8 };
                    entries.push(BeatGridEntry::new(beat as u16 + 1, beat_type, ts));
                    beat += 1;
                }
                let raw: Vec<u8> = entries.iter()
                    .flat_map(|e| e.to_bytes().unwrap_or_default())
                    .collect();
                let total_bg = raw.len() as u32;
                let n = raw.chunks(2400).count().max(1) as u32;
                ld.beat_grid_packets = if raw.is_empty() {
                    vec![Data::BeatGrid(BeatGridHeader::new_packet(lid, 0, 1, 0, vec![]))]
                } else {
                    raw.chunks(2400)
                        .enumerate()
                        .map(|(i, chunk)| Data::BeatGrid(BeatGridHeader::new_packet(
                            lid, total_bg, n, i as u32, chunk.to_vec())))
                        .collect()
                };
            }

            // Cue: default at start.
            ld.cue_packet = Some(Data::Cue(CueData::new(lid, 0)));

            // Artwork: empty placeholder.
            ld.artwork_packets = vec![Data::ArtworkFile(ArtworkFileData::new_packet(lid, 0, 1, 0, vec![]))];
        }

        // Populate last_metrics so stopped tracks can respond to MetricsData requests.
        let _ = self.update_metrics(layer);

        let status = self.inner.lock().unwrap().build_status_packet();
        self.send_broadcast(status)?;
        self.update_meta(layer)
    }

    pub fn unload_track(&mut self, layer: LayerId) -> Result<(), SendError> {
        {
            let mut inner = self.inner.lock().unwrap();
            *inner.layer_mut(layer) = LayerSnapshot::default();
        }
        let data = self.inner.lock().unwrap().build_status_packet();
        self.send_broadcast(data)
    }

    // --- position / tempo ---

    pub fn set_position_ms(&mut self, layer: LayerId, ms: u32) -> Result<(), SendError> {
        {
            let mut inner = self.inner.lock().unwrap();
            let l = inner.layer_mut(layer);
            l.position_ms = ms;
            l.current_time_ms = ms;
        }
        self.update_metrics(layer)
    }

    pub fn set_bpm(&mut self, layer: LayerId, bpm: f32) -> Result<(), SendError> {
        self.inner.lock().unwrap().layer_mut(layer).bpm = Bpm((bpm * 100.0) as u32);
        self.update_metrics(layer)
    }

    pub fn set_speed(&mut self, layer: LayerId, speed: Speed) -> Result<(), SendError> {
        self.inner.lock().unwrap().layer_mut(layer).speed = speed;
        self.update_metrics(layer)
    }

    pub fn set_sync_master(&mut self, layer: LayerId, master: bool) -> Result<(), SendError> {
        self.inner.lock().unwrap().layer_mut(layer).sync_master = master;
        self.update_metrics(layer)
    }

    // --- mixer ---

    pub fn set_master_fader(&mut self, level: u8) -> Result<(), SendError> {
        self.inner.lock().unwrap().mixer.master_fader_level = level;
        self.update_mixer()
    }

    pub fn set_crossfader(&mut self, pos: u8) -> Result<(), SendError> {
        self.inner.lock().unwrap().mixer.crossfader = pos;
        self.update_mixer()
    }

    pub fn set_channel_fader(&mut self, ch: usize, level: u8) -> Result<(), SendError> {
        if ch < 6 { self.inner.lock().unwrap().mixer.channels[ch].fader_level = level; }
        self.update_mixer()
    }

    pub fn set_channel_trim(&mut self, ch: usize, level: u8) -> Result<(), SendError> {
        if ch < 6 { self.inner.lock().unwrap().mixer.channels[ch].trim_level = level; }
        self.update_mixer()
    }

    pub fn set_channel_eq(&mut self, ch: usize, hi: u8, mid: u8, low: u8) -> Result<(), SendError> {
        if ch < 6 {
            let mut inner = self.inner.lock().unwrap();
            inner.mixer.channels[ch].eq_hi = hi;
            inner.mixer.channels[ch].eq_hi_mid = mid;
            inner.mixer.channels[ch].eq_low = low;
        }
        self.update_mixer()
    }

    pub fn set_channel_filter(&mut self, ch: usize, val: u8) -> Result<(), SendError> {
        if ch < 6 { self.inner.lock().unwrap().mixer.channels[ch].filter_color = val; }
        self.update_mixer()
    }

    pub fn set_channel_cue(&mut self, ch: usize, cue_a: bool, cue_b: bool) -> Result<(), SendError> {
        if ch < 6 {
            let mut inner = self.inner.lock().unwrap();
            inner.mixer.channels[ch].cue_a = cue_a;
            inner.mixer.channels[ch].cue_b = cue_b;
        }
        self.update_mixer()
    }

    pub fn set_on_air(&mut self, layer: LayerId, on_air: u8) {
        self.inner.lock().unwrap().layer_mut(layer).on_air = on_air;
    }

    // --- state read access ---

    pub fn layers(&self) -> Vec<LayerSnapshot> {
        self.inner.lock().unwrap().layers.clone()
    }

    pub fn mixer(&self) -> MixerSnapshot {
        self.inner.lock().unwrap().mixer.clone()
    }

    pub fn channel_snapshot(&self, ch: usize) -> ChannelSnapshot {
        self.inner.lock().unwrap().mixer.channels[ch.min(5)].clone()
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
        if i + 2 > buf.len() { break; }
        buf[i] = (ch & 0xff) as u8;
        buf[i + 1] = (ch >> 8) as u8;
        i += 2;
    }
}
