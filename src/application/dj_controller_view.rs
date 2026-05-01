//! Passive observer view of a DJ controller system connected via a Pro DJ Link → TCNet bridge.
//!
//! A bridge (e.g. a software running on the same machine as rekordbox) translates
//! Pioneer Pro DJ Link packets into TCNet packets.  This view consumes those TCNet
//! packets and assembles a coherent snapshot of all eight layer slots.

use std::collections::HashMap;
use kanal::Receiver;
use crate::application::ApplicationMessage;
use crate::application::domain::{Bpm, LayerId, LayerState, Speed, SmpteMode};
use crate::node::tcnet_packet_serde::Data;

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
pub struct DjControllerView {
    pub layers: HashMap<LayerId, LayerSnapshot>,
    rx: Receiver<ApplicationMessage>,
    tx: kanal::Sender<ApplicationMessage>,
}

impl DjControllerView {
    pub fn new((rx, tx): (Receiver<ApplicationMessage>, kanal::Sender<ApplicationMessage>)) -> Self {
        let mut layers = HashMap::new();
        for id in LayerId::ALL {
            layers.insert(id, LayerSnapshot::default());
        }
        Self { layers, rx, tx }
    }

    /// Drain all pending packets from the channel without blocking.
    /// Call this regularly (e.g. every frame or on a timer).
    pub fn process_available(&mut self) {
        while let Ok(Some(packet)) = self.rx.try_recv_realtime() {
            self.apply(packet);
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
