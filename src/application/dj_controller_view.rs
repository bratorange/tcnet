//! Passive observer view of a DJ controller system connected via a Pro DJ Link → TCNet bridge.
//!
//! A bridge (e.g. a software running on the same machine as rekordbox) translates
//! Pioneer Pro DJ Link packets into TCNet packets.  This view consumes those TCNet
//! packets and assembles a coherent snapshot of all eight layer slots.

use std::collections::HashMap;
use kanal::Receiver;
use crate::application::ApplicationMessage;
use crate::application::domain::{Bpm, LayerId, LayerState, Speed, SmpteMode};
use crate::node::tcnet_packet_serde::{Data, MixerChannel};


// ---------------------------------------------------------------------------
// Mixer master snapshot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct MixerSnapshot {
    pub mixer_id: u8,
    pub mixer_type: u8,
    pub mixer_name: String,

    // Master section
    pub master_audio_level: u8,
    pub master_fader_level: u8,
    pub master_filter: u8,
    pub master_cue_a: bool,
    pub master_cue_b: bool,
    pub link_cue_a: bool,
    pub link_cue_b: bool,

    // Master isolator
    pub isolator_on: bool,
    pub isolator_hi: u8,
    pub isolator_mid: u8,
    pub isolator_low: u8,

    // Master filter
    pub filter_hpf: u8,
    pub filter_lpf: u8,
    pub filter_resonance: u8,

    // Mic
    pub mic_eq_hi: u8,
    pub mic_eq_low: u8,

    // Crossfader
    pub crossfader: u8,
    pub crossfader_curve: u8,
    pub channel_fader_curve: u8,

    // Send FX
    pub send_fx_effect: u8,
    pub send_fx_level: u8,
    pub send_fx_time: u8,
    pub send_fx_size_feedback: u8,
    pub send_fx_hpf: u8,
    pub send_fx_master_mix: bool,

    // BeatFX
    pub beat_fx_on: bool,
    pub beat_fx_level_depth: u8,
    pub beat_fx_channel_select: u8,
    pub beat_fx_select: u8,

    // Headphones
    pub headphones_pre_eq: bool,
    pub headphones_a_level: u8,
    pub headphones_a_mix: u8,
    pub headphones_b_level: u8,
    pub headphones_b_mix: u8,

    // Booth
    pub booth_level: u8,
    pub booth_eq_hi: u8,
    pub booth_eq_low: u8,

    // Six channels (indices 0–5 = channels 1–6)
    pub channels: [ChannelSnapshot; 6],
}


/// State of one mixer channel.
#[derive(Debug, Clone, Default)]
pub struct ChannelSnapshot {
    /// Source selection (0=USB-A, 1=USB-B, 2=Digital, 3=Line, 4=Phono, etc.)
    pub source_select: u8,
    /// Current audio level (VU meter), 0–255.
    pub audio_level: u8,
    /// Fader position, 0–255.
    pub fader_level: u8,
    /// Trim/gain level, 0–255.
    pub trim_level: u8,
    /// Compressor level, 0–255.
    pub comp_level: u8,
    /// EQ high band, 0–255.
    pub eq_hi: u8,
    /// EQ high-mid band, 0–255.
    pub eq_hi_mid: u8,
    /// EQ low-mid band, 0–255.
    pub eq_low_mid: u8,
    /// EQ low band, 0–255.
    pub eq_low: u8,
    /// Filter/color knob, 0–255.
    pub filter_color: u8,
    /// Send FX level, 0–255.
    pub send: u8,
    /// CUE A state (0=off, 1=on).
    pub cue_a: bool,
    /// CUE B state (0=off, 1=on).
    pub cue_b: bool,
    /// Crossfader assignment (0=Thru, 1=A, 2=B).
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

impl MixerSnapshot {
    /// Returns the indices of all channels whose CUE A is active.
    pub fn cue_a_channels(&self) -> Vec<usize> {
        self.channels
            .iter()
            .enumerate()
            .filter(|(_, ch)| ch.cue_a)
            .map(|(i, _)| i + 1)
            .collect()
    }

    /// Returns indices of channels currently on-air (fader > 0).
    pub fn on_air_channels(&self) -> Vec<usize> {
        self.channels
            .iter()
            .enumerate()
            .filter(|(_, ch)| ch.is_on())
            .map(|(i, _)| i + 1)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Layer snapshot
// ---------------------------------------------------------------------------

/// Complete real-time state of one CDJ/layer slot.
#[derive(Debug, Clone, Default)]
pub struct LayerSnapshot {
    // --- From Status packet (updated ~1 Hz) ---
    pub source: u8,
    pub track_id: u32,
    pub name: String,

    // --- From Metrics packet (updated on change) ---
    pub state: LayerState,
    pub sync_master: bool,
    pub beat_marker: u8,       // 1–4
    pub track_length_ms: u32,
    pub position_ms: u32,
    pub speed: Speed,
    pub beat_number: u32,
    pub bpm: Bpm,
    pub pitch_bend: u16,

    // --- From Time packet (updated 1–40 ms) ---
    pub current_time_ms: u32,
    pub total_time_ms: u32,
    pub on_air: u8,            // 0 = off-air, >0 = fader value

    // --- From Metadata packet (updated on track change) ---
    pub artist: String,
    pub title: String,
    pub track_key: u16,

    // --- From TimeCode fields ---
    pub smpte_mode: SmpteMode,
    pub tc_state: u8,          // 0=Stopped 1=Running 2=ForceResync
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
// DJ Controller view
// ---------------------------------------------------------------------------

/// Passive view of all eight TCNet layers, kept up to date by consuming packets.
///
/// Typical use: call `process_available()` in your event loop or a dedicated
/// async task, then read `layers` whenever you need the current state.
#[derive(Debug)]
pub struct DjControllerView {
    pub layers: HashMap<LayerId, LayerSnapshot>,
    pub mixer_state: MixerSnapshot,
    rx: Receiver<ApplicationMessage>,
    tx: kanal::Sender<ApplicationMessage>,
}

impl DjControllerView {
    pub fn new((rx, tx): (Receiver<ApplicationMessage>, kanal::Sender<ApplicationMessage>)) -> Self {
        let mut layers = HashMap::new();
        let mixer_state = MixerSnapshot::default();
        for id in LayerId::ALL {
            layers.insert(id, LayerSnapshot::default());
        }
        Self { layers, rx, tx, mixer_state }
    }

    /// Drain all pending packets from the channel without blocking.
    /// Call this regularly (e.g. every frame or on a timer).
    pub fn process_available(&mut self) {
        while let Ok(Some(packet)) = self.rx.try_recv_realtime() {
            self.apply(packet);
            log::trace!("New Controller state: {:?}", self);
        }
    }

    fn apply(&mut self, packet: ApplicationMessage) {
        match packet.data {
            // ---------------------------------------------------------------
            // Status packet: source, track-id, layer name (~1 Hz)
            // ---------------------------------------------------------------
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
                    let snap = self.layers.entry(id).or_default();
                    snap.source = sources[i];
                    snap.track_id = track_ids[i];
                    snap.name = ascii_bytes_to_string(names[i]);
                }
            }

            // ---------------------------------------------------------------
            // Metrics packet: playback state, position, BPM (on change)
            // ---------------------------------------------------------------
            Data::Metrics(m) => {
                let layer_id = match LayerId::from_packet_id(m.layer_id) {
                    Some(id) => id,
                    None => return,
                };
                let snap = self.layers.entry(layer_id).or_default();
                snap.state = LayerState::from_u8(m.layer_state);
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

            // ---------------------------------------------------------------
            // Time packet: current position for all layers at high rate
            // ---------------------------------------------------------------
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
                    let snap = self.layers.entry(id).or_default();
                    snap.current_time_ms = cur_times[i];
                    snap.total_time_ms = tot_times[i];
                    snap.state = LayerState::from_u8(states[i]);
                    snap.on_air = on_air[i];
                    snap.beat_marker = beat_markers[i];

                    let tc = timecodes[i];
                    snap.smpte_mode = SmpteMode::from_u8(
                        if tc.smpte_mode == 0 { t.smpte_mode } else { tc.smpte_mode }
                    );
                    snap.tc_state = tc.state;
                    snap.tc_hours = tc.hours;
                    snap.tc_minutes = tc.minutes;
                    snap.tc_seconds = tc.seconds;
                    snap.tc_frames = tc.frames;
                }
            }

            // ---------------------------------------------------------------
            // Metadata packet: artist + title (on track change)
            // ---------------------------------------------------------------
            Data::Meta(m) => {
                let layer_id = match LayerId::from_packet_id(m.layer_id) {
                    Some(id) => id,
                    None => return,
                };
                let snap = self.layers.entry(layer_id).or_default();
                // v3.5+ encodes UTF-16LE in 256 bytes = up to 128 chars
                snap.artist = utf16le_to_string(&m.track_artist);
                snap.title = utf16le_to_string(&m.track_title);
                snap.track_key = m.track_key;
                snap.track_id = m.track_id;
            }

            Data::Mixer(m) => {
                let s = &mut self.mixer_state;
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

    /// Get the layer that is currently the sync master, if any.
    pub fn sync_master_layer(&self) -> Option<LayerId> {
        LayerId::ALL
            .iter()
            .find(|&&id| self.layers.get(&id).map_or(false, |l| l.sync_master))
            .copied()
    }

    /// Get all layers currently playing (playing or looping).
    pub fn playing_layers(&self) -> Vec<LayerId> {
        LayerId::ALL
            .iter()
            .filter(|&&id| {
                self.layers
                    .get(&id)
                    .map_or(false, |l| l.state.is_playing())
            })
            .copied()
            .collect()
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
