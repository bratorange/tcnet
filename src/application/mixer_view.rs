//! Passive view of a DJ mixer connected via TCNet (or a Pro DJ Link bridge).
//!
//! Consumes TCNet Mixer Data packets (msg 200, data type 150) and maintains
//! a live snapshot of the full mixer state: master section, six channels,
//! effects, and routing.

use kanal::Receiver;
use crate::application::ApplicationMessage;
use crate::node::tcnet_packet_serde::{Data, MixerChannel};

// ---------------------------------------------------------------------------
// Channel snapshot
// ---------------------------------------------------------------------------

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
// Mixer view
// ---------------------------------------------------------------------------

/// Passive view of the mixer state. Feed it packets with `process_available()`.
pub struct MixerView {
    pub state: MixerSnapshot,
    rx: Receiver<ApplicationMessage>,
}

impl MixerView {
    pub fn new(rx: Receiver<ApplicationMessage>) -> Self {
        Self {
            state: MixerSnapshot::default(),
            rx,
        }
    }

    /// Drain all pending packets without blocking.
    pub fn process_available(&mut self) {
        while let Ok(Some(packet)) = self.rx.try_recv() {
            self.apply(packet);
        }
    }

    fn apply(&mut self, packet: ApplicationMessage) {
        if let Data::Mixer(m) = packet.data {
            let s = &mut self.state;
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
    }
}
