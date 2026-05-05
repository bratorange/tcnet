use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use crate::node::tcnet_packet::{Data, Packet};
use crate::node::tcnet_packet_serde::{
    ArtworkFileData, BigWaveformData, Bpm, LayerId, LayerState, MixerChannel,
    RequestData, RequestDataType, SmallWaveformData, SmpteMode, Speed,
};

// ---------------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct MixerSnapshot {
    pub mixer_id: u8,
    pub mixer_type: u8,
    pub mixer_name: String,

    pub master_audio_level: u8,
    pub master_fader_level: u8,
    pub master_filter: u8,
    pub master_cue_a: bool,
    pub master_cue_b: bool,
    pub link_cue_a: bool,
    pub link_cue_b: bool,

    pub isolator_on: bool,
    pub isolator_hi: u8,
    pub isolator_mid: u8,
    pub isolator_low: u8,

    pub filter_hpf: u8,
    pub filter_lpf: u8,
    pub filter_resonance: u8,

    pub mic_eq_hi: u8,
    pub mic_eq_low: u8,

    pub crossfader: u8,
    pub crossfader_curve: u8,
    pub channel_fader_curve: u8,

    pub send_fx_effect: u8,
    pub send_fx_level: u8,
    pub send_fx_time: u8,
    pub send_fx_size_feedback: u8,
    pub send_fx_hpf: u8,
    pub send_fx_master_mix: bool,

    pub beat_fx_on: bool,
    pub beat_fx_level_depth: u8,
    pub beat_fx_channel_select: u8,
    pub beat_fx_select: u8,

    pub headphones_pre_eq: bool,
    pub headphones_a_level: u8,
    pub headphones_a_mix: u8,
    pub headphones_b_level: u8,
    pub headphones_b_mix: u8,

    pub booth_level: u8,
    pub booth_eq_hi: u8,
    pub booth_eq_low: u8,

    pub channels: [ChannelSnapshot; 6],
}

impl MixerSnapshot {
    pub fn cue_a_channels(&self) -> Vec<usize> {
        self.channels.iter().enumerate()
            .filter(|(_, ch)| ch.cue_a)
            .map(|(i, _)| i + 1)
            .collect()
    }

    pub fn on_air_channels(&self) -> Vec<usize> {
        self.channels.iter().enumerate()
            .filter(|(_, ch)| ch.is_on())
            .map(|(i, _)| i + 1)
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChannelSnapshot {
    pub source_select: u8,
    pub audio_level: u8,
    pub fader_level: u8,
    pub trim_level: u8,
    pub comp_level: u8,
    pub eq_hi: u8,
    pub eq_hi_mid: u8,
    pub eq_low_mid: u8,
    pub eq_low: u8,
    pub filter_color: u8,
    pub send: u8,
    pub cue_a: bool,
    pub cue_b: bool,
    pub crossfader_assign: u8,
}

impl ChannelSnapshot {
    pub fn is_on(&self) -> bool {
        self.fader_level > 0
    }
}

impl From<&MixerChannel> for ChannelSnapshot {
    fn from(ch: &MixerChannel) -> Self {
        Self {
            source_select: ch.source_select,
            audio_level: ch.audio_level,
            fader_level: ch.fader_level,
            trim_level: ch.trim_level,
            comp_level: ch.comp_level,
            eq_hi: ch.eq_hi_level,
            eq_hi_mid: ch.eq_hi_mid_level,
            eq_low_mid: ch.eq_low_mid_level,
            eq_low: ch.eq_low_level,
            filter_color: ch.filter_color,
            send: ch.send,
            cue_a: ch.cue_a != 0,
            cue_b: ch.cue_b != 0,
            crossfader_assign: ch.crossfader_assign,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LayerSnapshot {
    pub source: u8,
    pub track_id: u32,
    pub name: String,

    pub state: LayerState,
    pub sync_master: bool,
    pub beat_marker: u8,
    pub track_length_ms: u32,
    pub position_ms: u32,
    pub speed: Speed,
    pub beat_number: u32,
    pub bpm: Bpm,
    pub pitch_bend: u16,

    pub current_time_ms: u32,
    pub total_time_ms: u32,
    pub on_air: u8,

    pub artist: String,
    pub title: String,
    pub track_key: u16,

    pub smpte_mode: SmpteMode,
    pub tc_state: u8,
    pub tc_hours: u8,
    pub tc_minutes: u8,
    pub tc_seconds: u8,
    pub tc_frames: u8,
}

impl LayerSnapshot {
    pub fn is_on_air(&self) -> bool {
        self.on_air > 0
    }
}

// ---------------------------------------------------------------------------
// Triple buffer payload
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct DjControllerState {
    pub layers: Vec<LayerSnapshot>,
    pub mixer: MixerSnapshot,
}

impl Default for DjControllerState {
    fn default() -> Self {
        Self {
            layers: LayerId::ALL.iter().map(|_| LayerSnapshot::default()).collect(),
            mixer: MixerSnapshot::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct TimeoutError;

// ---------------------------------------------------------------------------
// Internal channel types
// ---------------------------------------------------------------------------

pub(crate) enum UserRequest {
    SmallWaveform {
        layer: LayerId,
        reply: oneshot::Sender<Result<SmallWaveformData, TimeoutError>>,
    },
    BigWaveform {
        layer: LayerId,
        reply: oneshot::Sender<Result<BigWaveformData, TimeoutError>>,
    },
    ArtworkFile {
        layer: LayerId,
        reply: oneshot::Sender<Result<ArtworkFileData, TimeoutError>>,
    },
}

pub(crate) struct OutgoingRequest {
    pub destination: Ipv4Addr,
    pub unicast_port: u16,
    pub data: Data,
}

// ---------------------------------------------------------------------------
// DjController — held by ForeignNode
// ---------------------------------------------------------------------------

pub(crate) struct DjController {
    pub packet_tx: kanal::Sender<Packet>,
    pub request_tx: kanal::Sender<UserRequest>,
    /// Moved into DjControllerView on first call to get_controller_view().
    pub buf_output: Option<triple_buffer::Output<DjControllerState>>,
}

impl DjController {
    pub fn new(
        outgoing_tx: kanal::Sender<OutgoingRequest>,
        foreign_addr: Ipv4Addr,
        unicast_port: u16,
    ) -> (Self, impl std::future::Future<Output = ()>) {
        let (packet_tx, packet_rx) = kanal::bounded::<Packet>(100);
        let (request_tx, request_rx) = kanal::bounded::<UserRequest>(16);
        let (buf_input, buf_output) =
            triple_buffer::triple_buffer(&DjControllerState::default());

        let fut = dj_controller_task(
            packet_rx,
            request_rx,
            outgoing_tx,
            buf_input,
            foreign_addr,
            unicast_port,
        );

        let ctrl = DjController {
            packet_tx,
            request_tx,
            buf_output: Some(buf_output),
        };
        (ctrl, fut)
    }
}

// ---------------------------------------------------------------------------
// Pending request tracking
// ---------------------------------------------------------------------------

enum PendingReply {
    SmallWaveform(oneshot::Sender<Result<SmallWaveformData, TimeoutError>>),
    BigWaveform(oneshot::Sender<Result<BigWaveformData, TimeoutError>>),
    ArtworkFile(oneshot::Sender<Result<ArtworkFileData, TimeoutError>>),
}

struct PendingRequest {
    layer: LayerId,
    deadline: Instant,
    reply: PendingReply,
}

fn fire_timeout(pending: &mut Vec<PendingRequest>) {
    let now = Instant::now();
    let mut i = 0;
    while i < pending.len() {
        if pending[i].deadline <= now {
            let p = pending.swap_remove(i);
            match p.reply {
                PendingReply::SmallWaveform(tx) => { let _ = tx.send(Err(TimeoutError)); }
                PendingReply::BigWaveform(tx)   => { let _ = tx.send(Err(TimeoutError)); }
                PendingReply::ArtworkFile(tx)   => { let _ = tx.send(Err(TimeoutError)); }
            }
        } else {
            i += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Update task
// ---------------------------------------------------------------------------

async fn dj_controller_task(
    packet_rx: kanal::Receiver<Packet>,
    request_rx: kanal::Receiver<UserRequest>,
    outgoing_tx: kanal::Sender<OutgoingRequest>,
    mut buf_input: triple_buffer::Input<DjControllerState>,
    foreign_addr: Ipv4Addr,
    unicast_port: u16,
) {
    let mut layers: HashMap<LayerId, LayerSnapshot> = {
        let mut m = HashMap::new();
        for id in LayerId::ALL {
            m.insert(id, LayerSnapshot::default());
        }
        m
    };
    let mut mixer = MixerSnapshot::default();
    let mut pending: Vec<PendingRequest> = Vec::new();

    loop {
        // --- drain incoming packets ---
        let mut packets = Vec::new();
        let _ = packet_rx.drain_into(&mut packets);

        for packet in packets {
            // check if this packet satisfies a pending request
            match &packet.data {
                Data::SmallWaveform(w) => {
                    if let Some(layer) = LayerId::from_packet_id(w.layer_id) {
                        let mut i = 0;
                        while i < pending.len() {
                            if pending[i].layer == layer
                                && matches!(pending[i].reply, PendingReply::SmallWaveform(_))
                            {
                                let p = pending.swap_remove(i);
                                if let PendingReply::SmallWaveform(tx) = p.reply {
                                    let _ = tx.send(Ok(w.clone()));
                                }
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
                Data::BigWaveform(w) => {
                    if let Some(layer) = LayerId::from_packet_id(w.layer_id) {
                        let mut i = 0;
                        while i < pending.len() {
                            if pending[i].layer == layer
                                && matches!(pending[i].reply, PendingReply::BigWaveform(_))
                            {
                                let p = pending.swap_remove(i);
                                if let PendingReply::BigWaveform(tx) = p.reply {
                                    let _ = tx.send(Ok(w.clone()));
                                }
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
                Data::ArtworkFile(a) => {
                    if let Some(layer) = LayerId::from_packet_id(a.layer_id) {
                        let mut i = 0;
                        while i < pending.len() {
                            if pending[i].layer == layer
                                && matches!(pending[i].reply, PendingReply::ArtworkFile(_))
                            {
                                let p = pending.swap_remove(i);
                                if let PendingReply::ArtworkFile(tx) = p.reply {
                                    let _ = tx.send(Ok(a.clone()));
                                }
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
                _ => {}
            }

            apply_packet(packet, &mut layers, &mut mixer);
        }

        // write updated state to triple buffer; order matches LayerId::ALL
        buf_input.write(DjControllerState {
            layers: LayerId::ALL.iter()
                .map(|id| layers.get(id).cloned().unwrap_or_default())
                .collect(),
            mixer: mixer.clone(),
        });

        // --- drain user requests ---
        let mut requests = Vec::new();
        let _ = request_rx.drain_into(&mut requests);

        for req in requests {
            match req {
                UserRequest::SmallWaveform { layer, reply } => {
                    let _ = outgoing_tx.send(OutgoingRequest {
                        destination: foreign_addr,
                        unicast_port,
                        data: Data::Request(RequestData {
                            data_type: RequestDataType::SmallWaveformData,
                            layer,
                        }),
                    });
                    pending.push(PendingRequest {
                        layer,
                        deadline: Instant::now() + Duration::from_secs(5),
                        reply: PendingReply::SmallWaveform(reply),
                    });
                }
                UserRequest::BigWaveform { layer, reply } => {
                    let _ = outgoing_tx.send(OutgoingRequest {
                        destination: foreign_addr,
                        unicast_port,
                        data: Data::Request(RequestData {
                            data_type: RequestDataType::LargeWaveformData,
                            layer,
                        }),
                    });
                    pending.push(PendingRequest {
                        layer,
                        deadline: Instant::now() + Duration::from_secs(5),
                        reply: PendingReply::BigWaveform(reply),
                    });
                }
                UserRequest::ArtworkFile { layer, reply } => {
                    let _ = outgoing_tx.send(OutgoingRequest {
                        destination: foreign_addr,
                        unicast_port,
                        data: Data::Request(RequestData {
                            data_type: RequestDataType::LowResArtworkFile,
                            layer,
                        }),
                    });
                    pending.push(PendingRequest {
                        layer,
                        deadline: Instant::now() + Duration::from_secs(5),
                        reply: PendingReply::ArtworkFile(reply),
                    });
                }
            }
        }

        // expire timed-out pending requests
        fire_timeout(&mut pending);

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// ---------------------------------------------------------------------------
// Packet → state apply logic (ported from DjControllerView::apply)
// ---------------------------------------------------------------------------

fn apply_packet(
    packet: Packet,
    layers: &mut HashMap<LayerId, LayerSnapshot>,
    mixer: &mut MixerSnapshot,
) {
    match packet.data {
        Data::Status(s) => {
            let sources = [
                s.layer_1_source, s.layer_2_source, s.layer_3_source, s.layer_4_source,
                s.layer_a_source, s.layer_b_source, s.layer_m_source, s.layer_c_source,
            ];
            let track_ids = [
                s.layer_1_track_id, s.layer_2_track_id, s.layer_3_track_id, s.layer_4_track_id,
                s.layer_a_track_id, s.layer_b_track_id, s.layer_m_track_id, s.layer_c_track_id,
            ];
            let names = [
                &s.layer_1_name, &s.layer_2_name, &s.layer_3_name, &s.layer_4_name,
                &s.layer_a_name, &s.layer_b_name, &s.layer_m_name, &s.layer_c_name,
            ];
            for id in LayerId::ALL {
                let i = id.index();
                let snap = layers.entry(id).or_default();
                snap.source = sources[i];
                snap.track_id = track_ids[i];
                snap.name = ascii_bytes_to_string(names[i]);
            }
        }

        Data::Metrics(m) => {
            let layer_id = match LayerId::from_packet_id(m.layer_id) {
                Some(id) => id,
                None => return,
            };
            let snap = layers.entry(layer_id).or_default();
            snap.state = m.layer_state;
            snap.sync_master = m.sync_master != 0;
            snap.beat_marker = m.beat_marker;
            snap.track_length_ms = m.track_length;
            snap.position_ms = m.current_position;
            snap.speed = Speed(m.speed);
            snap.beat_number = m.beat_number;
            snap.bpm = Bpm(m.bpm);
            snap.pitch_bend = m.pitch_bend;
            snap.track_id = m.track_id;
        }

        Data::Time(t) => {
            let cur_times = [
                t.l1_time, t.l2_time, t.l3_time, t.l4_time,
                t.la_time, t.lb_time, t.lm_time, t.lc_time,
            ];
            let tot_times = [
                t.l1_total_time, t.l2_total_time, t.l3_total_time, t.l4_total_time,
                t.la_total_time, t.lb_total_time, t.lm_total_time, t.lc_total_time,
            ];
            let states = [
                t.l1_layer_state, t.l2_layer_state, t.l3_layer_state, t.l4_layer_state,
                t.la_layer_state, t.lb_layer_state, t.lm_layer_state, t.lc_layer_state,
            ];
            let on_air = [
                t.l1_on_air, t.l2_on_air, t.l3_on_air, t.l4_on_air,
                t.la_on_air, t.lb_on_air, t.lm_on_air, t.lc_on_air,
            ];
            let beat_markers = [
                t.l1_beat_marker, t.l2_beat_marker, t.l3_beat_marker, t.l4_beat_marker,
                t.la_beat_marker, t.lb_beat_marker, t.lm_beat_marker, t.lc_beat_marker,
            ];
            let timecodes = [
                &t.l1_timecode, &t.l2_timecode, &t.l3_timecode, &t.l4_timecode,
                &t.la_timecode, &t.lb_timecode, &t.lm_timecode, &t.lc_timecode,
            ];

            for id in LayerId::ALL {
                let i = id.index();
                let snap = layers.entry(id).or_default();
                snap.current_time_ms = cur_times[i];
                snap.total_time_ms = tot_times[i];
                snap.state = states[i];
                snap.on_air = on_air[i];
                snap.beat_marker = beat_markers[i];

                let tc = timecodes[i];
                snap.smpte_mode = SmpteMode::from_u8(
                    if tc.smpte_mode == 0 { t.smpte_mode } else { tc.smpte_mode },
                );
                snap.tc_state = tc.state;
                snap.tc_hours = tc.hours;
                snap.tc_minutes = tc.minutes;
                snap.tc_seconds = tc.seconds;
                snap.tc_frames = tc.frames;
            }
        }

        Data::Meta(m) => {
            let layer_id = match LayerId::from_packet_id(m.layer_id) {
                Some(id) => id,
                None => return,
            };
            let snap = layers.entry(layer_id).or_default();
            snap.artist = utf16le_to_string(&m.track_artist);
            snap.title = utf16le_to_string(&m.track_title);
            snap.track_key = m.track_key;
            snap.track_id = m.track_id;
        }

        Data::Mixer(m) => {
            let s = mixer;
            s.mixer_id = m.mixer_id;
            s.mixer_type = m.mixer_type;
            s.mixer_name = std::str::from_utf8(&m.mixer_name)
                .unwrap_or("")
                .trim_end_matches('\0')
                .to_owned();

            s.master_audio_level = m.master_audio_level;
            s.master_fader_level = m.master_fader_level;
            s.master_filter = m.master_filter;
            s.master_cue_a = m.master_cue_a != 0;
            s.master_cue_b = m.master_cue_b != 0;
            s.link_cue_a = m.link_cue_a != 0;
            s.link_cue_b = m.link_cue_b != 0;

            s.isolator_on = m.master_isolator_on_off != 0;
            s.isolator_hi = m.master_isolator_hi;
            s.isolator_mid = m.master_isolator_mid;
            s.isolator_low = m.master_isolator_low;

            s.filter_hpf = m.filter_hpf;
            s.filter_lpf = m.filter_lpf;
            s.filter_resonance = m.filter_resonance;

            s.mic_eq_hi = m.mic_eq_hi;
            s.mic_eq_low = m.mic_eq_low;

            s.crossfader = m.cross_fader;
            s.crossfader_curve = m.cross_fader_curve;
            s.channel_fader_curve = m.channel_fader_curve;

            s.send_fx_effect = m.send_fx_effect;
            s.send_fx_level = m.send_fx_level;
            s.send_fx_time = m.send_fx_time;
            s.send_fx_size_feedback = m.send_fx_size_feedback;
            s.send_fx_hpf = m.send_fx_hpf;
            s.send_fx_master_mix = m.send_fx_master_mix != 0;

            s.beat_fx_on = m.beat_fx_on_off != 0;
            s.beat_fx_level_depth = m.beat_fx_level_depth;
            s.beat_fx_channel_select = m.beat_fx_channel_select;
            s.beat_fx_select = m.beat_fx_select;

            s.headphones_pre_eq = m.headphones_pre_eq != 0;
            s.headphones_a_level = m.headphones_a_level;
            s.headphones_a_mix = m.headphones_a_mix;
            s.headphones_b_level = m.headphones_b_level;
            s.headphones_b_mix = m.headphones_b_mix;

            s.booth_level = m.booth_level;
            s.booth_eq_hi = m.booth_eq_hi;
            s.booth_eq_low = m.booth_eq_low;

            for (i, ch) in m.channels.iter().enumerate() {
                if i < 6 {
                    s.channels[i] = ChannelSnapshot::from(ch);
                }
            }
        }

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ascii_bytes_to_string(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes)
        .unwrap_or("")
        .trim_end_matches('\0')
        .trim()
        .to_owned()
}

fn utf16le_to_string(bytes: &[u8]) -> String {
    let u16_chars: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&u16_chars)
}