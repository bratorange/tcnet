//! Wire-format types for the TCNet UDP protocol.
//!
//! Every struct in this module serialises and deserialises via [`deku`] and
//! mirrors a packet layout described in the TCNet specification:
//! <https://www.tc-supply.com/_files/ugd/b1c714_0b351a4099c14e738f0cd7fcea623265.pdf>
//!
//! Library users normally don't construct these types directly — they're built
//! and consumed inside [`TCNetClient`](crate::TCNetClient),
//! [`ActiveDJNode`](crate::ActiveDJNode) and [`DjControllerView`](crate::DjControllerView).
//! They're public so that observers can match on raw packet contents and so
//! that the message-type catalogue is part of the documented API surface.
//!
//! # Message-type catalogue
//!
//! Each TCNet packet starts with a 32-byte [`ManagementHeader`]. The header's
//! `message_type` byte (and, for message type 200, the first byte of the
//! payload) selects the payload struct:
//!
//! | Message type | (Data type) | Direction        | Struct                        |
//! |--------------|-------------|------------------|-------------------------------|
//! | 2            | —           | broadcast (60000) | [`OptInData`]                 |
//! | 3            | —           | broadcast (60000) | [`OptOutData`]                |
//! | 5            | —           | broadcast        | [`StatusData`]                |
//! | 10           | —           | unicast          | [`TimeSyncData`]              |
//! | 13           | —           | unicast          | [`ErrorNotificationData`]     |
//! | 20           | —           | unicast          | [`RequestData`]               |
//! | 30 / 213     | —           | unicast / bcast  | [`AppSpecificData`]           |
//! | 101          | —           | unicast          | [`ControlData`]               |
//! | 128          | —           | unicast          | [`TextData`]                  |
//! | 132          | —           | unicast          | [`KeyboardData`]              |
//! | 200          | 2           | unicast          | [`MetricsData`]               |
//! | 200          | 4           | unicast          | [`MetaData`]                  |
//! | 200          | 8           | unicast          | [`BeatGridHeader`] + [`BeatGridEntry`] |
//! | 200          | 12          | unicast          | [`CueData`] + [`CueEntry`]    |
//! | 200          | 16          | unicast          | [`SmallWaveformData`]         |
//! | 200          | 32          | unicast          | [`BigWaveformData`]           |
//! | 200          | 128         | unicast          | [`ArtworkFileData`]           |
//! | 200          | 150         | unicast          | [`MixerData`] + [`MixerChannel`] |
//! | 254          | —           | broadcast (60001) | [`TimePacketData`] + [`LayerTimecode`] |

use bitflags::bitflags;
use deku::ctx::Order;
use deku::prelude::{Reader, Writer};
use deku::{DekuError, DekuRead, DekuReader, DekuWrite, DekuWriter};
use std::fmt::{Debug, Formatter};
use std::io::{Read, Seek, Write};

/// 16-bit identifier carried in every [`ManagementHeader`].
pub type NodeId = u16;

bitflags! {
    /// Capability / behaviour bitflags reported by every node in its
    /// [`ManagementHeader`].
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NodeOptions: u16 {
        /// Authentication is required for extended communication.
        const NEED_AUTHENTICATION = 1;
        /// Node listens for TCNet Control Messages (TCNCM).
        const SUPPORTS_TCNCM = 2;
        /// Node listens for TCNet Application-Specific Data Packets (TCNASDP).
        const SUPPORTS_TCNASDP = 4;
        /// "Do not disturb" — node is idle/sleeping and will request data
        /// itself when needed (peers should reduce unsolicited traffic).
        const DND = 8;
    }
}

impl DekuWriter for NodeOptions {
    fn to_writer<W: Write + Seek>(&self, writer: &mut Writer<W>, _: ()) -> Result<(), DekuError> {
        writer.write_bytes(&self.bits().to_le_bytes())
    }
}

impl DekuReader<'_> for NodeOptions {
    fn from_reader_with_ctx<R: Read + Seek>(
        reader: &mut Reader<R>,
        _: (),
    ) -> Result<Self, DekuError>
    where
        Self: Sized,
    {
        let mut buf = [0u8; 2];
        let _ = reader.read_bytes(2, &mut buf, Order::Lsb0);
        let bits = u16::from_le_bytes(buf);
        Ok(NodeOptions::from_bits(bits).unwrap())
    }
}

/// Microsecond timestamp carried in [`ManagementHeader`].
pub type Timestamp = u32;
/// 8-byte ASCII node name (alias for [`AsciiString<8>`]).
pub type NodeName = AsciiString<8>;

/// `N` bytes of spec-reserved padding.
///
/// Preserved on the wire (zero-filled on writes), ignored on reads. The
/// `Debug` impl deliberately omits these bytes to keep packet dumps readable.
#[derive(PartialEq, DekuWrite, DekuRead, Clone)]
pub struct ReservedData<const N: usize>(pub [u8; N]);

impl<const N: usize> Default for ReservedData<N> {
    fn default() -> Self {
        ReservedData([0; N])
    }
}

impl<const N: usize> Debug for ReservedData<N> {
    fn fmt(&self, _: &mut Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// Role this node plays on the TCNet network.
///
/// `Master` drives the clock for slaves; `Slave` follows; `Auto` lets the
/// network elect a master automatically; `Repeater` bridges segments.
#[derive(Debug, PartialEq, Clone, Copy, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum NodeType {
    /// Let the network decide which node becomes the master.
    Auto = 1,
    /// This node drives the clock.
    Master = 2,
    /// This node follows the master's clock.
    Slave = 4,
    /// Bridges packets between network segments.
    Repeater = 8,
}

/// Carried in [`StatusData`] when a node is configured for automatic master
/// election. Only one variant is currently defined by the spec.
#[derive(Debug, PartialEq, Clone, Copy, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum AutoMasterMode {
    /// Default / unspecified.
    Variant = 0,
}

/// Fixed-size ASCII identifier carried in many TCNet packets.
///
/// Unused tail bytes are conventionally left as zero. The
/// [`std::fmt::Display`] impl renders the bytes as a UTF-8 string (assuming
/// the field is well-formed ASCII per spec). Build one from a string literal
/// with the [`into_ascii!`](crate::into_ascii) macro.
#[derive(PartialEq, Clone, Copy, DekuRead, DekuWrite)]
pub struct AsciiString<const N: usize>(pub [u8; N]);

/// Build an [`AsciiString<N>`] from a string literal at compile time.
///
/// The literal's byte length determines `N`; any unused tail bytes default to
/// zero. Useful for filling fixed-size identifier fields in
/// [`ApplicationConfig`](crate::ApplicationConfig):
///
/// ```
/// use tcnet::into_ascii;
/// let name = into_ascii!("MyApp__________");
/// ```
#[macro_export]
macro_rules! into_ascii {
    ($str:literal) => {{
        use $crate::protocol::AsciiString;
        const N: usize = $str.len();
        let mut arr: [u8; N] = [0; N];
        arr[..N].copy_from_slice($str.as_bytes());
        AsciiString(arr)
    }};
}

impl<const N: usize> std::fmt::Display for AsciiString<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", std::str::from_utf8(&self.0).unwrap())
    }
}

impl<const N: usize> Debug for AsciiString<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self)
    }
}

/// All eight TCNet layer slots a DJ controller node can expose.
///
/// The spec defines layers `L1`–`L4` as the main playback decks, `LA` / `LB`
/// as sample-player layers, `LM` as the microphone layer, and `LC` as an
/// auxiliary / cue layer. Wire-encoded as the 1-based packet IDs listed below;
/// see also [`LayerId::index`] for 0-based array indexing.
#[derive(Debug, PartialEq, Eq, Clone, Copy, DekuRead, DekuWrite, Hash)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum LayerId {
    /// Main playback deck 1.
    L1 = 1,
    /// Main playback deck 2.
    L2 = 2,
    /// Main playback deck 3.
    L3 = 3,
    /// Main playback deck 4.
    L4 = 4,
    /// Sample-player layer A.
    LA = 5,
    /// Sample-player layer B.
    LB = 6,
    /// Microphone layer.
    LM = 7,
    /// Auxiliary / cue layer.
    LC = 8,
}

impl LayerId {
    /// All eight layer IDs in the order they appear in [`TimePacketData`] and
    /// [`StatusData`].
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

    /// The 1-based numeric layer ID as it appears on the wire (`L1`=1 … `LC`=8).
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

    /// Convert a 1-based wire-format layer ID back into a [`LayerId`].
    /// Returns `None` for any value outside `1..=8`.
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

    /// 0-based index into the layer-shaped arrays returned by
    /// [`DjControllerView::get_layers`](crate::DjControllerView::get_layers)
    /// and used internally for [`TimePacketData`] / [`StatusData`] field lookups.
    pub fn index(self) -> usize {
        self.as_packet_id() as usize - 1
    }
}

/// Current playhead state of a single layer, as transmitted in
/// [`MetricsData`] and [`TimePacketData`].
#[derive(Debug, PartialEq, Clone, Copy, DekuRead, DekuWrite, Default)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum LayerState {
    /// No track loaded — the default for empty layers.
    #[default]
    Idle = 0,
    /// Track is loaded and the playhead is advancing.
    Playing = 3,
    /// Playing inside a loop.
    Looping = 4,
    /// Paused at a position.
    Paused = 5,
    /// Stopped at position 0.
    Stopped = 6,
    /// Cue button held — playing while held, returns to cue on release.
    CueButtonDown = 7,
    /// Platter touched (CDJ jog wheel pressed).
    PlatterDown = 8,
    /// Fast-forward search.
    FastForward = 9,
    /// Fast-reverse search.
    FastReverse = 10,
    /// Hold (vinyl-style pause).
    Hold = 11,
    /// Any value not enumerated above is preserved here.
    #[deku(id_pat = "_")]
    Unknown(u8),
}

impl LayerState {
    /// `true` if the layer is currently advancing the playhead
    /// (`Playing` or `Looping`).
    pub fn is_playing(self) -> bool {
        matches!(self, LayerState::Playing | LayerState::Looping)
    }
}

/// SMPTE frame-rate identifiers carried in [`LayerTimecode`] and
/// [`TimePacketData::smpte_mode`].
///
/// `Fps2997` represents 29.97 drop-frame timecode.
#[derive(Debug, PartialEq, Clone, Copy, DekuRead, DekuWrite, Default)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum SmpteMode {
    /// 24 fps (film).
    #[default]
    Fps24 = 24,
    /// 25 fps (PAL).
    Fps25 = 25,
    /// 29.97 fps drop-frame (NTSC).
    Fps2997 = 29,
    /// 30 fps (non-drop).
    Fps30 = 30,
}

impl SmpteMode {
    /// Decode a raw SMPTE-mode byte. Unknown values fall back to `Fps25`.
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

/// Playback speed as transmitted in [`MetricsData`].
///
/// Encoded as an unsigned integer where `32768` = 100% (normal speed),
/// `0` = stopped, `65536` = 200%. Use [`Speed::as_percent`] for a floating-point
/// percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Speed(pub u32);

impl Speed {
    /// Normal playback speed (100% / 32768).
    pub const NORMAL: Speed = Speed(32768);
    /// Stopped (0 / 0%).
    pub const STOPPED: Speed = Speed(0);

    /// Return the speed as a percentage (`100.0` = normal speed).
    pub fn as_percent(self) -> f32 {
        self.0 as f32 / 327.68
    }
}

/// BPM as transmitted in [`MetricsData`].
///
/// Stored as `BPM × 100` in an unsigned integer to avoid floating-point on the
/// wire (e.g. 134.00 BPM is encoded as `13400`). Use [`Bpm::as_f32`] for a
/// floating-point view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bpm(pub u32);

impl Bpm {
    /// Return the BPM as a floating-point value.
    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 100.0
    }
}

/// Data category requested from a peer through a [`RequestData`] packet
/// (message type 20).
///
/// Each variant maps to a specific response packet category — for example,
/// asking for `BeatGridData` triggers one or more [`BeatGridHeader`] packets
/// from the peer.
#[derive(Debug, PartialEq, Clone, Copy, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum RequestDataType {
    /// Request a fresh [`MetricsData`] packet.
    MetricsData = 2,
    /// Request a fresh [`MetaData`] packet.
    MetaData = 4,
    /// Request a (possibly multi-packet) [`BeatGridHeader`] response.
    BeatGridData = 8,
    /// Request a [`CueData`] response.
    CueData = 12,
    /// Request a [`SmallWaveformData`] response (single packet, 2400 bytes).
    SmallWaveformData = 16,
    /// Request a [`BigWaveformData`] response (multi-packet, ~4400 B per chunk).
    LargeWaveformData = 32,
    /// Request a low-resolution artwork JPEG ([`ArtworkFileData`]).
    LowResArtworkFile = 128,
    /// Request the current [`MixerData`] snapshot.
    MixerData = 150,
}

/// 32-byte header that precedes every TCNet packet.
///
/// Identifies the sender, the protocol version, the message type, and a
/// monotonically increasing sequence number used for ordering. The `_header`
/// field is always the ASCII bytes `"TCN"` — receivers reject packets where
/// that magic is missing.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct ManagementHeader {
    /// Sender's 16-bit node identifier.
    pub node_id: NodeId,
    /// TCNet protocol major version (this crate emits `3`).
    pub protocol_version_major: u8,
    /// TCNet protocol minor version (this crate emits `6`).
    pub protocol_version_minor: u8,
    /// Always the literal ASCII bytes `"TCN"` — packets without this magic are dropped.
    pub _header: AsciiString<3>,
    /// Message type identifier (e.g. 2 = OptIn, 5 = Status, 200 = data packet, 254 = Time).
    pub message_type: u8,
    /// Short human-readable name for the sender.
    pub node_name: NodeName,
    /// Sequence number — incremented per outgoing packet, wraps at 256.
    pub seq: u8,
    /// Role of the sender (Master / Slave / Auto / Repeater).
    pub node_type: NodeType,
    /// Capability bitflags of the sender.
    pub node_options: NodeOptions,
    /// Microsecond timestamp of the sender at packet emission.
    pub timestamp: Timestamp,
}

/// Discovery announcement (message type 2).
///
/// Broadcast every second on UDP port 60000 and unicast to every known peer.
/// Carries the sender's identifying metadata so peers can register it. The
/// dispatcher in this crate handles OptIn traffic automatically.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct OptInData {
    pub node_count: u16,               // Amount of Registered Node
    pub node_listener_port: u16,       // Listener Port for Unicast Messages
    pub uptime: u16,                   // Uptime of Node in SEC
    pub _reserved0: ReservedData<2>,   // RESERVED
    pub vendor_name: AsciiString<16>,  // Vendor
    pub application: AsciiString<16>,  // Application / Device Name
    pub application_major_version: u8, // Application/Device Major Version
    pub application_minor_version: u8, // Application/Device Minor Version
    pub application_bug_version: u8,   // Application/Device Minor Version
    pub _reserved1: ReservedData<1>,   // RESERVED
}

/// Per-layer status byte carried in [`StatusData`].
///
/// Only one variant is currently defined by the spec; future protocol
/// revisions may extend this enum.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum LayerStatus {
    /// Default / unspecified.
    Variant = 0,
}

/// Periodic status broadcast (message type 5).
///
/// Broadcast on UDP port 60000 ≈ once per second. Carries each layer's source
/// device, the loaded track ID, and the human-readable layer name —
/// effectively a directory of what each layer is currently doing.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct StatusData {
    pub node_count: u16,                  // Amount of Registered Nodes
    pub node_listener_port: u16,          // Listener Port for Unicast Messages
    pub _reserved0: [u8; 6],              // RESERVED
    pub layer_1_source: u8,               // Layer 1 Source
    pub layer_2_source: u8,               // Layer 2 Source
    pub layer_3_source: u8,               // Layer 3 Source
    pub layer_4_source: u8,               // Layer 4 Source
    pub layer_a_source: u8,               // Layer A Source
    pub layer_b_source: u8,               // Layer B Source
    pub layer_m_source: u8,               // Layer M Source
    pub layer_c_source: u8,               // Layer C Source
    pub layer_1_status: LayerStatus,      // Layer 1 Status
    pub layer_2_status: LayerStatus,      // Layer 2 Status
    pub layer_3_status: LayerStatus,      // Layer 3 Status
    pub layer_4_status: LayerStatus,      // Layer 4 Status
    pub layer_a_status: LayerStatus,      // Layer A Status
    pub layer_b_status: LayerStatus,      // Layer B Status
    pub layer_m_status: LayerStatus,      // Layer M Status
    pub layer_c_status: LayerStatus,      // Layer C Status
    pub layer_1_track_id: u32,            // Assigned Track ID for Layer 1
    pub layer_2_track_id: u32,            // Assigned Track ID for Layer 2
    pub layer_3_track_id: u32,            // Assigned Track ID for Layer 3
    pub layer_4_track_id: u32,            // Assigned Track ID for Layer 4
    pub layer_a_track_id: u32,            // Assigned Track ID for Layer A
    pub layer_b_track_id: u32,            // Assigned Track ID for Layer B
    pub layer_m_track_id: u32,            // Assigned Track ID for Layer M
    pub layer_c_track_id: u32,            // Assigned Track ID for Layer C
    pub _reserved1: ReservedData<1>,      // RESERVED
    pub smpte_mode: u8,                   // SMPTE Mode
    pub auto_master_mode: AutoMasterMode, // Auto Master Mode
    pub _reserved2: ReservedData<15>,     // RESERVED
    pub app_specific: [u8; 72],           // APP SPECIFIC
    pub layer_1_name: [u8; 16],           // Layer 1 Source
    pub layer_2_name: [u8; 16],           // Layer 2 Source
    pub layer_3_name: [u8; 16],           // Layer 3 Source
    pub layer_4_name: [u8; 16],           // Layer 4 Source
    pub layer_a_name: [u8; 16],           // Layer A Source
    pub layer_b_name: [u8; 16],           // Layer B Source
    pub layer_m_name: [u8; 16],           // Layer M Source
    pub layer_c_name: [u8; 16],           // Layer C Source
}

/// Departure announcement (message type 3).
///
/// Sent by a node leaving the network so peers can drop it from their
/// discovered-nodes set immediately rather than waiting for the 10-second
/// timeout.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct OptOutData {
    node_count: u16,         // Amount of Registered Nodes
    node_listener_port: u16, // Listener Port for Unicast Messages
}

/// Clock synchronisation handshake (message type 10).
///
/// `step = 0` initiates the handshake; `step = 1` is the peer's response.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct TimeSyncData {
    step: u8,                    // Step No (0=Initialize, 1=Response)
    _reserved0: ReservedData<1>, // RESERVED
    node_listener_port: u16,     // Listener Port for Unicast Messages
    remote_timestamp: u32,       // Timestamp of Remote Node
}

/// Error / notification packet (message type 13).
///
/// Sent in reply to a malformed or unsatisfiable [`RequestData`].
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct ErrorNotificationData {
    datatype: RequestDataType, // Data type of Request
    layer_id: u8,              // Layer ID of original request
    code: u16,                 // Returned Code
    message_type: u16,         // Message type of Request
}

/// On-demand data request (message type 20).
///
/// Sent from one node to another to ask for a specific category of data
/// ([`RequestDataType`]) for a given [`LayerId`]. The dispatcher in this crate
/// answers these for an [`ActiveDJNode`](crate::ActiveDJNode) from its
/// pre-built response cache.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct RequestData {
    /// The category of data being requested.
    pub data_type: RequestDataType,
    /// The layer the data is associated with.
    pub layer: LayerId,
}

/// Control message (message type 101).
///
/// Carries an ASCII control path (path-like string). Used by TCNet Control
/// Message-capable nodes (`SUPPORTS_TCNCM` in [`NodeOptions`]).
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct ControlData {
    step: u8,                    // Step No (0=Initialize, 1=Response)
    _reserved0: ReservedData<1>, // RESERVED
    #[deku(endian = "little")]
    data_size: u32, // Total Data Size
    _reserved1: ReservedData<12>, // RESERVED
    #[deku(count = "data_size")]
    control_path: Vec<u8>, // String with Control Path (ASCII TEXT)
}

/// Text payload (message type 128).
///
/// Carries an arbitrary ASCII text string of length `data_size`.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct TextData {
    step: u8,                    // Step No (0=Initialize, 1=Response)
    _reserved0: ReservedData<1>, // RESERVED
    #[deku(endian = "little")]
    data_size: u32, // Total Data Size
    _reserved1: ReservedData<12>, // RESERVED
    #[deku(count = "data_size")]
    text_data: Vec<u8>, // String Text Data (ASCII TEXT)
}

/// Keyboard input passthrough (message type 132).
///
/// Two bytes of HEX-ASCII keyboard scan-code data.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct KeyboardData {
    _reserved0: ReservedData<1>, // RESERVED
    _reserved1: ReservedData<1>, // RESERVED
    #[deku(endian = "little")]
    data_size: u32, // Total Data Size
    _reserved2: ReservedData<12>, // RESERVED
    keyboard_data: [u8; 2],      // Keyboard Data (HEX ASCII Code)
}

/// Periodic per-layer playback metrics (message type 200, data type 2).
///
/// Emitted ~20× per second per playing layer. Contains the live values that
/// change frequently: layer state, beat marker, track length, current
/// position, speed, beat number, BPM, pitch bend.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct MetricsData {
    pub data_type: u8,               // Datatype 2 = Metrics
    pub layer_id: u8,                // Layer Number
    pub _reserved0: ReservedData<1>, // RESERVED
    pub layer_state: LayerState,     // Layer State
    pub _reserved1: ReservedData<1>, // RESERVED
    pub sync_master: u8,             // Sync Master
    pub _reserved2: ReservedData<1>, // RESERVED
    pub beat_marker: u8,             // Beat Marker
    #[deku(endian = "little")]
    pub track_length: u32, // Track Length in Milliseconds
    #[deku(endian = "little")]
    pub current_position: u32, // Play head Position in Milliseconds
    #[deku(endian = "little")]
    pub speed: u32, // Play head Speed
    pub _reserved3: ReservedData<13>, // RESERVED
    #[deku(endian = "little")]
    pub beat_number: u32, // Beat Number
    pub _reserved4: ReservedData<51>, // RESERVED
    #[deku(endian = "little")]
    pub bpm: u32, // BPM
    #[deku(endian = "little")]
    pub pitch_bend: u16, // Pitch Bend
    #[deku(endian = "little")]
    pub track_id: u32, // Assigned Track ID
}

/// Track metadata (message type 200, data type 4).
///
/// Emitted on track-change. From protocol v3.5 onwards `track_artist` and
/// `track_title` are encoded as **UTF-16 little-endian** in 256-byte fields
/// (a null `u16` terminates the string).
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct MetaData {
    pub data_type: u8,               // Datatype 4 = Metadata
    pub layer_id: u8,                // Layer ID
    pub _reserved0: ReservedData<1>, // RESERVED
    pub _reserved1: ReservedData<2>, // RESERVED
    /// Track artist, UTF-16 LE (null-`u16` terminated).
    pub track_artist: [u8; 256],
    /// Track title, UTF-16 LE (null-`u16` terminated).
    pub track_title: [u8; 256],
    #[deku(endian = "little")]
    pub track_key: u16, // Track KEY
    #[deku(endian = "little")]
    pub track_id: u32, // Assigned Track ID
}

/// Beat-grid response (message type 200, data type 8).
///
/// Each [`BeatGridHeader`] is one chunk of a possibly multi-packet response —
/// `packet_no` ranges over `0..total_packets` and `payload` carries 8-byte
/// serialised [`BeatGridEntry`] items. Reassemble by concatenating payloads in
/// `packet_no` order.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct BeatGridHeader {
    data_type: u8,    // Datatype 8 = Beat Grid Data
    pub layer_id: u8, // Layer Number
    #[deku(endian = "little")]
    pub data_size: u32, // Total Data Size (bytes of all entries combined)
    #[deku(endian = "little")]
    pub total_packets: u32, // Total Packets used for data
    #[deku(endian = "little")]
    pub packet_no: u32, // Packet Number
    #[deku(endian = "little")]
    pub data_cluster_size: u32, // Bytes of entry data in this packet
    /// Serialised [`BeatGridEntry`] items (each 8 bytes).
    #[deku(count = "data_cluster_size")]
    pub payload: Vec<u8>,
}

/// One entry in a beat grid: a labelled beat at a specific timestamp.
///
/// `beat_type` follows the TCNet convention `20 = downbeat`, `10 = upbeat`.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct BeatGridEntry {
    /// 1-based beat number within the track.
    #[deku(endian = "little")]
    pub beat_number: u16,
    /// `20` = downbeat (first beat of a bar), `10` = upbeat (any other beat).
    pub beat_type: u8,
    _reserved0: ReservedData<1>,
    /// Beat timestamp, milliseconds from track start.
    #[deku(endian = "little")]
    pub beat_timestamp: u32,
}

/// One hot-cue entry inside [`CueData`].
///
/// Holds the cue's type byte, start/end times in milliseconds and an RGB
/// colour. Up to 18 entries are carried per layer.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct CueEntry {
    cue_type: u8,                // Cue Type
    _reserved0: ReservedData<1>, // RESERVED
    #[deku(endian = "little")]
    cue_in_time: u32, // CUE IN Time
    #[deku(endian = "little")]
    cue_out_time: u32, // CUE OUT Time
    _reserved1: ReservedData<1>, // RESERVED
    cue_color: [u8; 3],          // CUE Color (R, G, B)
    _reserved2: ReservedData<8>, // RESERVED
}

/// Cue and loop information for a layer (message type 200, data type 12).
///
/// Carries up to 18 cue entries plus the current loop in/out points (in ms).
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct CueData {
    data_type: u8,                // Datatype 12 = Cue Data
    layer_id: u8,                 // Layer Number
    _reserved0: ReservedData<16>, // RESERVED
    #[deku(endian = "little")]
    loop_in: u32, // Loop IN Time
    #[deku(endian = "little")]
    loop_out: u32, // Loop OUT Time
    cues: [CueEntry; 18],         // CUE 1-18
}

/// Low-resolution full-track waveform (message type 200, data type 16).
///
/// Single-packet response carrying exactly 2400 bytes of waveform data.
/// The payload is encoded as 1200 byte-pairs `(level, colour)`:
///
/// * `level` (odd index) — amplitude, 0–255.
/// * `colour` (even index) — frequency band hint (`0x03` = blue/low,
///   `0x04` = green/mid, `0x05` = red/high) used by Pioneer-style coloured
///   waveform views.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct SmallWaveformData {
    data_type: u8,    // Datatype 16 = Small Waveform
    pub layer_id: u8, // Layer Number
    #[deku(endian = "little")]
    data_size: u32, // Total Datasize (2400)
    #[deku(endian = "little")]
    total_packets: u32, // Total Packets used for data
    #[deku(endian = "little")]
    packet_no: u32, // Packet Number
    _reserved0: ReservedData<4>, // RESERVED
    waveform_data: [u8; 2400], // BLevel (Odd Bytes) / BColor (Even Bytes)
}

/// High-resolution full-track waveform (message type 200, data type 32).
///
/// Multi-packet response, ~50 samples / second, split into chunks of
/// `data_cluster_size` bytes (typically 4400). Layout per byte-pair matches
/// [`SmallWaveformData`] (`level` + `colour`). Reassemble by concatenating
/// `waveform_data` of each chunk in `packet_no` order.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct BigWaveformData {
    data_type: u8,    // Datatype 32 = Big Waveform
    pub layer_id: u8, // Layer Number
    #[deku(endian = "little")]
    data_size: u32, // Total Data size
    #[deku(endian = "little")]
    total_packets: u32, // Total Packets used for data
    #[deku(endian = "little")]
    packet_no: u32, // Packet Number
    #[deku(endian = "little")]
    data_cluster_size: u32, // Data Cluster Size (standard: 4800)
    #[deku(count = "data_cluster_size")]
    waveform_data: Vec<u8>, // BLevel (Odd Bytes) / BColor (Even Bytes)
}

/// State of one mixer channel — a single fader strip on the physical mixer.
///
/// Embedded six times inside [`MixerData`].
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone, Copy)]
pub struct MixerChannel {
    pub source_select: u8,     // Channel Source Select
    pub audio_level: u8,       // Channel Audio Level
    pub fader_level: u8,       // Channel Fader Level
    pub trim_level: u8,        // Channel Trim Level
    pub comp_level: u8,        // Channel Compressor Level
    pub eq_hi_level: u8,       // Channel EQ Hi Level
    pub eq_hi_mid_level: u8,   // Channel EQ Hi Mid Level
    pub eq_low_mid_level: u8,  // Channel EQ Low Mid Level
    pub eq_low_level: u8,      // Channel EQ Low Level
    pub filter_color: u8,      // Channel Filter/Color
    pub send: u8,              // Channel FX Send
    pub cue_a: u8,             // Channel CUE A
    pub cue_b: u8,             // Channel CUE B
    pub crossfader_assign: u8, // Channel Crossfader Assign
    pub _reserved: [u8; 10],   // RESERVED
}

/// Mixer state snapshot (message type 200, data type 150).
///
/// Carries the master section, FX section and six [`MixerChannel`] strips.
/// All level/EQ/filter fields are 8-bit (0–255 range).
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct MixerData {
    pub data_type: u8,                   // Datatype 150 = Mixer Data
    pub mixer_id: u8,                    // Mixer ID
    pub mixer_type: u8,                  // Mixer Type
    pub _reserved0: ReservedData<1>,     // RESERVED
    pub _reserved1: ReservedData<1>,     // RESERVED
    pub mixer_name: [u8; 16],            // Name of Mixer
    pub _reserved2: ReservedData<12>,    // RESERVED
    pub _reserved3: ReservedData<2>,     // RESERVED FOR MIC 1-2 LEVEL
    pub mic_eq_hi: u8,                   // Mic EQ HI
    pub mic_eq_low: u8,                  // Mic EQ Low
    pub master_audio_level: u8,          // Master Audio Level
    pub master_fader_level: u8,          // Master Fader Level
    pub _reserved4: ReservedData<4>,     // RESERVED
    pub link_cue_a: u8,                  // Link CUE A
    pub link_cue_b: u8,                  // Link CUE B
    pub master_filter: u8,               // Master Filter
    pub _reserved5: ReservedData<1>,     // RESERVED
    pub master_cue_a: u8,                // Master CUE A
    pub master_cue_b: u8,                // Master CUE B
    pub _reserved6: ReservedData<1>,     // RESERVED
    pub master_isolator_on_off: u8,      // Master Isolator Switch
    pub master_isolator_hi: u8,          // Master Isolator Hi
    pub master_isolator_mid: u8,         // Master Isolator Mid
    pub master_isolator_low: u8,         // Master Isolator Low
    pub _reserved7: ReservedData<1>,     // RESERVED
    pub filter_hpf: u8,                  // Filter HPF
    pub filter_lpf: u8,                  // Filter LPF
    pub filter_resonance: u8,            // Filter Resonance
    pub _reserved8: ReservedData<2>,     // RESERVED
    pub send_fx_effect: u8,              // Send FX Effect
    pub send_fx_ext_1: u8,               // Send Return Ext 1
    pub send_fx_ext_2: u8,               // Send Return Ext 2
    pub send_fx_master_mix: u8,          // Send FX Master Mix
    pub send_fx_size_feedback: u8,       // Send FX Size Feedback
    pub send_fx_time: u8,                // Send FX Time
    pub send_fx_hpf: u8,                 // Send FX HPF
    pub send_fx_level: u8,               // Send FX Level
    pub send_return_3_source_select: u8, // Send Return 3 Source Select
    pub send_return_3_type: u8,          // Send Return 3 Type
    pub send_return_3_on_off: u8,        // Send Return 3 ON/OFF
    pub send_return_3_level: u8,         // Send Return 3 Level
    pub _reserved9: ReservedData<1>,     // RESERVED
    pub channel_fader_curve: u8,         // Channel Fader Curve
    pub cross_fader_curve: u8,           // Cross Fader Curve
    pub cross_fader: u8,                 // Cross Fader
    pub beat_fx_on_off: u8,              // BeatFX ON/OFF
    pub beat_fx_level_depth: u8,         // BeatFX Level/Depth
    pub beat_fx_channel_select: u8,      // BeatFX Channel Select
    pub beat_fx_select: u8,              // BeatFX Select
    pub beat_fx_freq_hi: u8,             // BeatFX Frequency Hi
    pub beat_fx_freq_mid: u8,            // BeatFX Frequency Mid
    pub beat_fx_freq_low: u8,            // BeatFX Frequency Low
    pub headphones_pre_eq: u8,           // Headphones Pre EQ
    pub headphones_a_level: u8,          // Headphones A Level
    pub headphones_a_mix: u8,            // Headphones A Mix
    pub headphones_b_level: u8,          // Headphones B Level
    pub headphones_b_mix: u8,            // Headphones B Mix
    pub booth_level: u8,                 // Booth Level
    pub booth_eq_hi: u8,                 // Booth EQ Hi
    pub booth_eq_low: u8,                // Booth EQ Low
    pub _reserved10: [u8; 10],           // RESERVED
    pub channels: [MixerChannel; 6],     // Channels 1-6
}

/// Low-resolution track artwork (message type 200/204, data type 128).
///
/// Multi-packet response carrying a raw JPEG payload split into
/// `data_cluster_size` byte chunks (typically 4400). Reassemble by
/// concatenating `file_data` in `packet_no` order; the result is a complete
/// JPEG file.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct ArtworkFileData {
    data_type: u8,    // Datatype 128 = Low Res Artwork File
    pub layer_id: u8, // Layer Number
    #[deku(endian = "little")]
    data_size: u32, // Total Data size
    #[deku(endian = "little")]
    total_packets: u32, // Total Packets used for data
    #[deku(endian = "little")]
    packet_no: u32, // Packet Number
    #[deku(endian = "little")]
    data_cluster_size: u32, // Data Cluster Size (standard: 4800)
    #[deku(count = "data_cluster_size")]
    file_data: Vec<u8>, // Raw JPEG file data
}

/// Application-specific data passthrough (message type 30 or 213).
///
/// Allows applications to ride on top of TCNet to exchange opaque payloads.
/// Sender and receiver agree on the meaning out-of-band; the protocol only
/// guarantees delivery and packet-level framing.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct AppSpecificData {
    data_identifier_1: u8, // Application Identifier Signature 1/2
    data_identifier_2: u8, // Application Identifier Signature 2/2
    #[deku(endian = "little")]
    data_size: u32, // Data Size of all packets
    #[deku(endian = "little")]
    total_packets: u32, // Total of all packets
    #[deku(endian = "little")]
    packet_no: u32, // Packet No
    #[deku(endian = "little")]
    packet_signature: u32, // Signature of Packet (178260640)
    #[deku(count = "data_size")]
    data: Vec<u8>, // Data
}

/// SMPTE timecode for a single layer (one row inside [`TimePacketData`]).
///
/// `state` follows: `0 = Stopped`, `1 = Running`, `2 = Force Resync`.
#[derive(Debug, Clone, Copy, PartialEq, DekuRead, DekuWrite)]
pub struct LayerTimecode {
    pub smpte_mode: u8, // Layer SMPTE Mode (24/25/29/30)
    pub state: u8,      // Time Code State (0=Stopped, 1=Running, 2=Force Resync)
    pub hours: u8,      // Time Code Hours (0-23)
    pub minutes: u8,    // Time Code Minutes (0-59)
    pub seconds: u8,    // Time Code Seconds (0-59)
    pub frames: u8,     // Time Code Frames
}

/// High-frequency timing broadcast (message type 254).
///
/// Broadcast on UDP port 60001 every ~20 ms. Carries, for each of the eight
/// layers: current play position (ms), total length (ms), beat marker, layer
/// state, SMPTE timecode, and the on-air "fader position" byte (0 = down,
/// 255 = fully up) that the mixer is reporting. This is the highest-frequency
/// packet on the network and the one most clients sync visuals to.
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct TimePacketData {
    #[deku(endian = "little")]
    pub l1_time: u32, // LAYER 1 Current Time in Milliseconds
    #[deku(endian = "little")]
    pub l2_time: u32, // LAYER 2 Current Time in Milliseconds
    #[deku(endian = "little")]
    pub l3_time: u32, // LAYER 3 Current Time in Milliseconds
    #[deku(endian = "little")]
    pub l4_time: u32, // LAYER 4 Current Time in Milliseconds
    #[deku(endian = "little")]
    pub la_time: u32, // LAYER A Current Time in Milliseconds
    #[deku(endian = "little")]
    pub lb_time: u32, // LAYER B Current Time in Milliseconds
    #[deku(endian = "little")]
    pub lm_time: u32, // LAYER M Current Time in Milliseconds
    #[deku(endian = "little")]
    pub lc_time: u32, // LAYER C Current Time in Milliseconds
    #[deku(endian = "little")]
    pub l1_total_time: u32, // LAYER 1 Total Time in Milliseconds
    #[deku(endian = "little")]
    pub l2_total_time: u32, // LAYER 2 Total Time in Milliseconds
    #[deku(endian = "little")]
    pub l3_total_time: u32, // LAYER 3 Total Time in Milliseconds
    #[deku(endian = "little")]
    pub l4_total_time: u32, // LAYER 4 Total Time in Milliseconds
    #[deku(endian = "little")]
    pub la_total_time: u32, // LAYER A Total Time in Milliseconds
    #[deku(endian = "little")]
    pub lb_total_time: u32, // LAYER B Total Time in Milliseconds
    #[deku(endian = "little")]
    pub lm_total_time: u32, // LAYER M Total Time in Milliseconds
    #[deku(endian = "little")]
    pub lc_total_time: u32, // LAYER C Total Time in Milliseconds
    pub l1_beat_marker: u8,          // Layer 1 Beatmarker
    pub l2_beat_marker: u8,          // Layer 2 Beatmarker
    pub l3_beat_marker: u8,          // Layer 3 Beatmarker
    pub l4_beat_marker: u8,          // Layer 4 Beatmarker
    pub la_beat_marker: u8,          // Layer A Beatmarker
    pub lb_beat_marker: u8,          // Layer B Beatmarker
    pub lm_beat_marker: u8,          // Layer M Beatmarker
    pub lc_beat_marker: u8,          // Layer C Beatmarker
    pub l1_layer_state: LayerState,  // Layer 1 Layer State
    pub l2_layer_state: LayerState,  // Layer 2 Layer State
    pub l3_layer_state: LayerState,  // Layer 3 Layer State
    pub l4_layer_state: LayerState,  // Layer 4 Layer State
    pub la_layer_state: LayerState,  // Layer A State
    pub lb_layer_state: LayerState,  // Layer B State
    pub lm_layer_state: LayerState,  // Layer M State
    pub lc_layer_state: LayerState,  // Layer C State
    pub _reserved0: ReservedData<1>, // RESERVED
    pub smpte_mode: u8,              // General SMPTE Mode
    pub l1_timecode: LayerTimecode,  // Layer 1 Timecode
    pub l2_timecode: LayerTimecode,  // Layer 2 Timecode
    pub l3_timecode: LayerTimecode,  // Layer 3 Timecode
    pub l4_timecode: LayerTimecode,  // Layer 4 Timecode
    pub la_timecode: LayerTimecode,  // Layer A Timecode
    pub lb_timecode: LayerTimecode,  // Layer B Timecode
    pub lm_timecode: LayerTimecode,  // Layer M Timecode
    pub lc_timecode: LayerTimecode,  // Layer C Timecode
    pub l1_on_air: u8,               // Layer 1 OnAir State (fader position 0-255)
    pub l2_on_air: u8,               // Layer 2 OnAir State
    pub l3_on_air: u8,               // Layer 3 OnAir State
    pub l4_on_air: u8,               // Layer 4 OnAir State
    pub la_on_air: u8,               // Layer A OnAir State
    pub lb_on_air: u8,               // Layer B OnAir State
    pub lm_on_air: u8,               // Layer M OnAir State
    pub lc_on_air: u8,               // Layer C OnAir State
}

// ---------------------------------------------------------------------------
// Constructors for building response packets
// ---------------------------------------------------------------------------

impl BeatGridEntry {
    /// Build one beat-grid entry. `beat_type = 20` marks a downbeat (first
    /// beat of a bar); any other value (typically `10`) marks an upbeat.
    pub fn new(beat_number: u16, beat_type: u8, beat_timestamp: u32) -> Self {
        Self {
            beat_number,
            beat_type,
            _reserved0: ReservedData::default(),
            beat_timestamp,
        }
    }
}

impl BeatGridHeader {
    /// Build one chunk of a multi-packet beat-grid response.
    ///
    /// * `layer_id` — the 1-based layer ID this grid belongs to.
    /// * `total_data_size` — total byte size of all entries combined across every chunk.
    /// * `total_packets` / `packet_no` — packet-of-N indexing (0-based `packet_no`).
    /// * `payload` — serialised [`BeatGridEntry`] items for this chunk (8 bytes each).
    pub fn new_packet(
        layer_id: u8,
        total_data_size: u32,
        total_packets: u32,
        packet_no: u32,
        payload: Vec<u8>,
    ) -> Self {
        let data_cluster_size = payload.len() as u32;
        Self {
            data_type: 8,
            layer_id,
            data_size: total_data_size,
            total_packets,
            packet_no,
            data_cluster_size,
            payload,
        }
    }
}

impl SmallWaveformData {
    /// Build a small-waveform response from 2400 bytes of `(level, colour)`
    /// pairs. See [`SmallWaveformData`] for the byte-pair layout.
    pub fn new(layer_id: u8, waveform_data: [u8; 2400]) -> Self {
        Self {
            data_type: 16,
            layer_id,
            data_size: 2400,
            total_packets: 1,
            packet_no: 0,
            _reserved0: ReservedData::default(),
            waveform_data,
        }
    }

    /// Borrow the raw 2400-byte waveform payload.
    pub fn bytes(&self) -> &[u8] {
        &self.waveform_data
    }
}

impl BigWaveformData {
    /// Build one chunk of a multi-packet big-waveform response.
    /// See [`BigWaveformData`] for the chunk layout.
    pub fn new_packet(
        layer_id: u8,
        total_size: u32,
        total_packets: u32,
        packet_no: u32,
        chunk: Vec<u8>,
    ) -> Self {
        let cluster = chunk.len() as u32;
        Self {
            data_type: 32,
            layer_id,
            data_size: total_size,
            total_packets,
            packet_no,
            data_cluster_size: cluster,
            waveform_data: chunk,
        }
    }

    /// Borrow this chunk's waveform payload.
    pub fn bytes(&self) -> &[u8] {
        &self.waveform_data
    }
}

impl ArtworkFileData {
    /// Build one chunk of a multi-packet artwork-file response.
    /// `chunk` carries part of a JPEG byte stream.
    pub fn new_packet(
        layer_id: u8,
        total_size: u32,
        total_packets: u32,
        packet_no: u32,
        chunk: Vec<u8>,
    ) -> Self {
        let cluster = chunk.len() as u32;
        Self {
            data_type: 128,
            layer_id,
            data_size: total_size,
            total_packets,
            packet_no,
            data_cluster_size: cluster,
            file_data: chunk,
        }
    }
}

impl CueEntry {
    /// Build one cue entry.
    ///
    /// * `cue_type` — cue type byte per spec (e.g. `1` for a hot cue).
    /// * `cue_in_time` / `cue_out_time` — start / end positions in ms.
    /// * `color` — RGB colour for the cue marker.
    pub fn new(cue_type: u8, cue_in_time: u32, cue_out_time: u32, color: [u8; 3]) -> Self {
        Self {
            cue_type,
            _reserved0: ReservedData::default(),
            cue_in_time,
            cue_out_time,
            _reserved1: ReservedData::default(),
            cue_color: color,
            _reserved2: ReservedData::default(),
        }
    }
    /// Sentinel "empty" entry (`cue_type = 0`, all times zero).
    pub const EMPTY: Self = Self {
        cue_type: 0,
        _reserved0: ReservedData([0; 1]),
        cue_in_time: 0,
        cue_out_time: 0,
        _reserved1: ReservedData([0; 1]),
        cue_color: [0; 3],
        _reserved2: ReservedData([0; 8]),
    };
}

impl CueData {
    /// Build a [`CueData`] response with up to one populated cue at `cue_in`.
    /// All other cue slots are filled with [`CueEntry::EMPTY`].
    pub fn new(layer_id: u8, cue_in: u32) -> Self {
        let mut cues = [CueEntry::EMPTY; 18];
        if cue_in > 0 {
            cues[0] = CueEntry::new(1, cue_in, cue_in, [255, 128, 0]);
        }
        Self {
            data_type: 12,
            layer_id,
            _reserved0: ReservedData::default(),
            loop_in: 0,
            loop_out: 0,
            cues,
        }
    }
}
