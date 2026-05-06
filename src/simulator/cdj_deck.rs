use std::fs::File;
use std::io::BufReader;
use std::time::Instant;
use rodio::{Decoder, Sink};
use crate::active_node::{ActiveDJNode, TrackMeta};
use crate::node::tcnet_packet_serde::{LayerId, LayerState};
use crate::simulator::audio::AudioEngine;
use crate::simulator::virtual_usb::TrackInfo;

pub struct CDJDeck {
    pub layer_id: LayerId,
    pub loaded_track: Option<TrackInfo>,
    pub waveform: Vec<f32>,
    pub cue_ms: u32,
    pub bpm: f32,

    sink: Option<Sink>,
    play_start: Option<Instant>,
    position_at_start: u32,
    paused_position: u32,
}

impl CDJDeck {
    pub fn new(layer_id: LayerId) -> Self {
        Self {
            layer_id,
            loaded_track: None,
            waveform: Vec::new(),
            cue_ms: 0,
            bpm: 120.0,
            sink: None,
            play_start: None,
            position_at_start: 0,
            paused_position: 0,
        }
    }

    pub fn load(&mut self, track: TrackInfo, engine: &AudioEngine, node: &mut ActiveDJNode) {
        self.sink = None;
        self.play_start = None;
        self.paused_position = 0;
        self.position_at_start = 0;
        self.cue_ms = 0;
        self.bpm = track.bpm.unwrap_or(120.0);
        self.waveform = placeholder_waveform(track.duration_ms);

        let meta = TrackMeta {
            title: track.title.clone(),
            artist: track.artist.clone(),
            duration_ms: track.duration_ms,
            bpm: self.bpm,
            track_id: simple_hash(&track.title),
        };
        let _ = node.load_track(self.layer_id, meta);

        let sink = engine.new_sink();
        sink.pause();
        if let Ok(file) = File::open(&track.path) {
            if let Ok(source) = Decoder::new(BufReader::new(file)) {
                sink.append(source);
            }
        }
        self.sink = Some(sink);
        self.loaded_track = Some(track);
    }

    pub fn play(&mut self, node: &mut ActiveDJNode) {
        if let Some(ref sink) = self.sink {
            if sink.is_paused() {
                sink.play();
            }
            self.position_at_start = self.paused_position;
            self.play_start = Some(Instant::now());
            let _ = node.play(self.layer_id);
        }
    }

    pub fn pause(&mut self, node: &mut ActiveDJNode) {
        self.paused_position = self.current_position_ms();
        if let Some(ref sink) = self.sink {
            sink.pause();
        }
        self.play_start = None;
        let _ = node.pause(self.layer_id);
    }

    pub fn stop(&mut self, node: &mut ActiveDJNode) {
        self.paused_position = 0;
        self.play_start = None;
        if let Some(ref sink) = self.sink {
            sink.pause();
        }
        let _ = node.stop(self.layer_id);
    }

    pub fn cue_press(&mut self, node: &mut ActiveDJNode) {
        // If playing, set cue point at current position. If stopped/paused, return to cue.
        if self.is_playing() {
            self.cue_ms = self.current_position_ms();
        } else {
            self.paused_position = self.cue_ms;
        }
        let _ = node.cue(self.layer_id, self.cue_ms);
    }

    pub fn set_tempo(&mut self, bpm: f32, node: &mut ActiveDJNode) {
        self.bpm = bpm;
        let _ = node.set_bpm(self.layer_id, bpm);
    }

    pub fn toggle_play_pause(&mut self, node: &mut ActiveDJNode) {
        if self.is_playing() {
            self.pause(node);
        } else {
            self.play(node);
        }
    }

    pub fn current_position_ms(&self) -> u32 {
        if let Some(start) = self.play_start {
            let elapsed = start.elapsed().as_millis() as u32;
            self.position_at_start.saturating_add(elapsed)
        } else {
            self.paused_position
        }
    }

    pub fn is_playing(&self) -> bool {
        self.play_start.is_some()
    }

    pub fn duration_ms(&self) -> u32 {
        self.loaded_track.as_ref().map(|t| t.duration_ms).unwrap_or(0)
    }

    /// Call each frame to sync position into the active node.
    pub fn tick(&mut self, node: &mut ActiveDJNode) {
        if self.is_playing() {
            let pos = self.current_position_ms();
            let dur = self.duration_ms();
            let _ = node.set_position_ms(self.layer_id, pos);
            // Auto-stop at end
            if dur > 0 && pos >= dur {
                self.paused_position = 0;
                self.play_start = None;
                let _ = node.stop(self.layer_id);
            }
        }
    }

    /// Returns the layer state for display.
    pub fn layer_state(&self) -> LayerState {
        if self.sink.is_none() {
            LayerState::Idle
        } else if self.is_playing() {
            LayerState::Playing
        } else {
            LayerState::Paused
        }
    }

    pub fn set_volume(&self, vol: f32) {
        if let Some(ref sink) = self.sink {
            sink.set_volume(vol.clamp(0.0, 1.0));
        }
    }

    /// Formatted time string MM:SS.
    pub fn format_time(ms: u32) -> String {
        let secs = ms / 1000;
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

fn placeholder_waveform(duration_ms: u32) -> Vec<f32> {
    let n = ((duration_ms / 200).max(100) as usize).min(2000);
    let mut v = 0.3f32;
    (0..n).map(|i| {
        let noise = ((i as f32 * 1.3).sin() * 0.15 + (i as f32 * 0.7).cos() * 0.1).abs();
        v = (v + noise).clamp(0.05, 0.95);
        v
    }).collect()
}

fn simple_hash(s: &str) -> u32 {
    s.bytes().fold(2166136261u32, |acc, b| {
        acc.wrapping_mul(16777619).wrapping_add(b as u32)
    })
}
