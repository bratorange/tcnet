/// All eight TCNet layer slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerId {
    L1,
    L2,
    L3,
    L4,
    LA,
    LB,
    LM,
    LC,
}

impl LayerId {
    /// Canonical list in TCNet spec order.
    pub const ALL: [LayerId; 8] = [
        LayerId::L1,
        LayerId::L2,
        LayerId::L3,
        LayerId::L4,
        LayerId::LA,
        LayerId::LB,
        LayerId::LM,
        LayerId::LC,
    ];

    /// The 1-based numeric ID used in packets (L1=1 … LC=8).
    pub fn as_packet_id(self) -> u8 {
        match self {
            LayerId::L1 => 1,
            LayerId::L2 => 2,
            LayerId::L3 => 3,
            LayerId::L4 => 4,
            LayerId::LA => 5,
            LayerId::LB => 6,
            LayerId::LM => 7,
            LayerId::LC => 8,
        }
    }

    pub fn from_packet_id(id: u8) -> Option<Self> {
        match id {
            1 => Some(LayerId::L1),
            2 => Some(LayerId::L2),
            3 => Some(LayerId::L3),
            4 => Some(LayerId::L4),
            5 => Some(LayerId::LA),
            6 => Some(LayerId::LB),
            7 => Some(LayerId::LM),
            8 => Some(LayerId::LC),
            _ => None,
        }
    }

    /// The 0-based index into arrays (TimePacketData, StatusData).
    pub fn index(self) -> usize {
        self.as_packet_id() as usize - 1
    }
}

/// Playhead state of a layer as defined in the TCNet spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayerState {
    #[default]
    Idle,
    Playing,
    Looping,
    Paused,
    Stopped,
    CueButtonDown,
    PlatterDown,
    FastForward,
    FastReverse,
    Hold,
    Unknown(u8),
}

impl LayerState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => LayerState::Idle,
            3 => LayerState::Playing,
            4 => LayerState::Looping,
            5 => LayerState::Paused,
            6 => LayerState::Stopped,
            7 => LayerState::CueButtonDown,
            8 => LayerState::PlatterDown,
            9 => LayerState::FastForward,
            10 => LayerState::FastReverse,
            11 => LayerState::Hold,
            other => LayerState::Unknown(other),
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            LayerState::Idle => 0,
            LayerState::Playing => 3,
            LayerState::Looping => 4,
            LayerState::Paused => 5,
            LayerState::Stopped => 6,
            LayerState::CueButtonDown => 7,
            LayerState::PlatterDown => 8,
            LayerState::FastForward => 9,
            LayerState::FastReverse => 10,
            LayerState::Hold => 11,
            LayerState::Unknown(v) => v,
        }
    }

    pub fn is_playing(self) -> bool {
        matches!(self, LayerState::Playing | LayerState::Looping)
    }
}

/// SMPTE frame rate values as defined in the spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmpteMode {
    #[default]
    Fps24 = 24,
    Fps25 = 25,
    Fps2997 = 29,
    Fps30 = 30,
}

impl SmpteMode {
    pub fn from_u8(v: u8) -> Self {
        match v {
            24 => SmpteMode::Fps24,
            25 => SmpteMode::Fps25,
            29 => SmpteMode::Fps2997,
            30 => SmpteMode::Fps30,
            _ => SmpteMode::Fps25,
        }
    }
}

/// Speed value as transmitted in MetricsData: 32768 = 100%, 0 = stopped, 65536 = 200%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Speed(pub u32);

impl Speed {
    pub const NORMAL: Speed = Speed(32768);
    pub const STOPPED: Speed = Speed(0);

    /// Returns the speed as a percentage (100.0 = normal).
    pub fn as_percent(self) -> f32 {
        self.0 as f32 / 327.68
    }
}

/// BPM as transmitted: stored as integer, representing BPM * 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bpm(pub u32);

impl Bpm {
    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 100.0
    }
}
