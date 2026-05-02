use std::fmt::{Debug, Formatter};
use std::io::{Read, Seek, Write};
use bitflags::bitflags;
use deku::{DekuError, DekuRead, DekuReader, DekuWrite, DekuWriter};
use deku::ctx::Order;
use deku::prelude::{Reader, Writer};

pub type NodeId = u16;

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NodeOptions: u16 {
        const NEED_AUTHENTICATION = 1;  // Authentication for extended communication needed
        const SUPPORTS_TCNCM = 2;       // Listens to TCNet Control Messages
        const SUPPORTS_TCNASDP = 4;     // Listens to TCNet Application Specific Data Packet
        const DND = 8;                  // Do not disturb/Sleeping. Node will request data itself if needed to avoid traffic
    }
}

impl DekuWriter for NodeOptions {
    fn to_writer<W: Write + Seek>(&self, writer: &mut Writer<W>, _: ()) -> Result<(), DekuError> {
        writer.write_bytes(&self.bits().to_le_bytes())
    }
}

impl DekuReader<'_> for NodeOptions {
    fn from_reader_with_ctx<R: Read + Seek>(reader: &mut Reader<R>, _: ()) -> Result<Self, DekuError>
    where
        Self: Sized
    {
        let mut buf = [0u8; 2];
        let _ = reader.read_bytes(2, &mut buf, Order::Lsb0);
        let bits = u16::from_le_bytes(buf);
        Ok(NodeOptions::from_bits(bits).unwrap())
    }
}

pub type Timestamp = u32; // timestamp in microseconds
pub type NodeName = AsciiString<8>;

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


#[derive(Debug, PartialEq, Clone, Copy, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum NodeType {
    Auto = 1,
    Master= 2,
    Slave = 4,
    Repeater = 8,
}

#[derive(Debug, PartialEq, Clone, Copy, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum AutoMasterMode{
    Variant = 0, // TODO
}

#[derive(PartialEq, Clone, Copy, DekuRead, DekuWrite)]
pub struct AsciiString<const N: usize>(pub [u8; N]);

#[macro_export]
macro_rules! into_ascii {
    ($str:literal) => {{
        use crate::node::tcnet_packet_serde::AsciiString;
        const N: usize = $str.len();
        let mut arr: [u8; N] = [0; N];
        arr[..N].copy_from_slice($str.as_bytes());
        AsciiString(arr)
    }};
}

impl<const N: usize> std::fmt::Display for AsciiString<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", std::str::from_utf8(&self.0).unwrap())
    }
}

impl<const N: usize> Debug for AsciiString<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self)
    }
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct ManagementHeader {
    pub node_id: NodeId,
    pub protocol_version_major: u8,
    pub protocol_version_minor: u8,
    pub _header: AsciiString<3>, // this just here for serde purposes and must allways be "TCN"
    pub message_type: u8,
    pub node_name: NodeName,
    pub seq: u8,
    pub node_type: NodeType,
    pub node_options: NodeOptions,
    pub timestamp: Timestamp,
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct OptInData{
    pub node_count: u16, // Amount of Registered Node
    pub node_listener_port: u16,            // Listener Port for Unicast Messages
    pub uptime: u16,                        // Uptime of Node in SEC
    pub _reserved0: ReservedData<2>,        // RESERVED
    pub vendor_name: AsciiString<16>,       // Vendor
    pub application: AsciiString<16>,       // Application / Device Name
    pub application_major_version: u8,      // Application/Device Major Version
    pub application_minor_version: u8,      // Application/Device Minor Version
    pub application_bug_version: u8,        // Application/Device Minor Version
    pub _reserved1: ReservedData<1>,        // RESERVED
}

// TODO Opt-Out packet

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum LayerStatus{
    Variant = 0, // TODO
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct StatusData {
    pub node_count: u16,                    // Amount of Registered Nodes
    pub node_listener_port: u16,            // Listener Port for Unicast Messages
    pub _reserved0:[u8; 6],                 // RESERVED
    pub layer_1_source: u8,                 // Layer 1 Source
    pub layer_2_source: u8,                 // Layer 2 Source
    pub layer_3_source: u8,                 // Layer 3 Source
    pub layer_4_source: u8,                 // Layer 4 Source
    pub layer_a_source: u8,                 // Layer A Source
    pub layer_b_source: u8,                 // Layer B Source
    pub layer_m_source: u8,                 // Layer M Source
    pub layer_c_source: u8,                 // Layer C Source
    pub layer_1_status: LayerStatus,        // Layer 1 Status
    pub layer_2_status: LayerStatus,        // Layer 2 Status
    pub layer_3_status: LayerStatus,        // Layer 3 Status
    pub layer_4_status: LayerStatus,        // Layer 4 Status
    pub layer_a_status: LayerStatus,        // Layer A Status
    pub layer_b_status: LayerStatus,        // Layer B Status
    pub layer_m_status: LayerStatus,        // Layer M Status
    pub layer_c_status: LayerStatus,        // Layer C Status
    pub layer_1_track_id: u32,              // Assigned Track ID for Layer 1
    pub layer_2_track_id: u32,              // Assigned Track ID for Layer 2
    pub layer_3_track_id: u32,              // Assigned Track ID for Layer 3
    pub layer_4_track_id: u32,              // Assigned Track ID for Layer 4
    pub layer_a_track_id: u32,              // Assigned Track ID for Layer A
    pub layer_b_track_id: u32,              // Assigned Track ID for Layer B
    pub layer_m_track_id: u32,              // Assigned Track ID for Layer M
    pub layer_c_track_id: u32,              // Assigned Track ID for Layer C
    pub _reserved1: ReservedData<1>,        // RESERVED
    pub smpte_mode: u8,                     // SMPTE Mode
    pub auto_master_mode: AutoMasterMode,   // Auto Master Mode
    pub _reserved2: ReservedData<15>,       // RESERVED
    pub app_specific: [u8; 72],             // APP SPECIFIC
    pub layer_1_name: [u8; 16],             // Layer 1 Source
    pub layer_2_name: [u8; 16],             // Layer 2 Source
    pub layer_3_name: [u8; 16],             // Layer 3 Source
    pub layer_4_name: [u8; 16],             // Layer 4 Source
    pub layer_a_name: [u8; 16],             // Layer A Source
    pub layer_b_name: [u8; 16],             // Layer B Source
    pub layer_m_name: [u8; 16],             // Layer M Source
    pub layer_c_name: [u8; 16],             // Layer C Source
}

// OPT-OUT (Message Type 3)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct OptOutData {
    node_count: u16,                    // Amount of Registered Nodes
    node_listener_port: u16,            // Listener Port for Unicast Messages
}

// TIME SYNC (Message Type 10)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct TimeSyncData {
    step: u8,                           // Step No (0=Initialize, 1=Response)
    _reserved0: ReservedData<1>,        // RESERVED
    node_listener_port: u16,            // Listener Port for Unicast Messages
    remote_timestamp: u32,              // Timestamp of Remote Node
}

// ERROR/NOTIFICATION (Message Type 13)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct ErrorNotificationData {
    datatype: u8,                       // Data type of Request
    layer_id: u8,                       // Layer ID of original request
    code: u16,                          // Returned Code
    message_type: u16,                  // Message type of Request
}

// REQUEST (Message Type 20)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct RequestData {
    data_type: u8,                      // Data Type
    layer: u8,                          // Layer where data belongs to
}

// CONTROL (Message Type 101)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct ControlData {
    step: u8,                           // Step No (0=Initialize, 1=Response)
    _reserved0: ReservedData<1>,                     // RESERVED
    #[deku(endian = "little")]
    data_size: u32,                     // Total Data Size
    _reserved1: ReservedData<12>,               // RESERVED
    #[deku(count = "data_size")]
    control_path: Vec<u8>,              // String with Control Path (ASCII TEXT)
}

// TEXT DATA (Message Type 128)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct TextData {
    step: u8,                           // Step No (0=Initialize, 1=Response)
    _reserved0: ReservedData<1>,                     // RESERVED
    #[deku(endian = "little")]
    data_size: u32,                     // Total Data Size
    _reserved1: ReservedData<12>,               // RESERVED
    #[deku(count = "data_size")]
    text_data: Vec<u8>,                 // String Text Data (ASCII TEXT)
}

// KEYBOARD DATA (Message Type 132)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct KeyboardData {
    _reserved0: ReservedData<1>,                     // RESERVED
    _reserved1: ReservedData<1>,                     // RESERVED
    #[deku(endian = "little")]
    data_size: u32,                     // Total Data Size
    _reserved2: ReservedData<12>,               // RESERVED
    keyboard_data: [u8; 2],             // Keyboard Data (HEX ASCII Code)
}

// DATA PACKET - METRICS DATA (Message Type 200, Data Type 2)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct MetricsData {
    pub data_type: u8,                      // Datatype 2 = Metrics
    pub layer_id: u8,                       // Layer Number
    pub _reserved0: ReservedData<1>,                     // RESERVED
    pub layer_state: u8,                    // Layer State
    pub _reserved1: ReservedData<1>,                     // RESERVED
    pub sync_master: u8,                    // Sync Master
    pub _reserved2: ReservedData<1>,                     // RESERVED
    pub beat_marker: u8,                    // Beat Marker
    #[deku(endian = "little")]
    pub track_length: u32,                  // Track Length in Milliseconds
    #[deku(endian = "little")]
    pub current_position: u32,              // Play head Position in Milliseconds
    #[deku(endian = "little")]
    pub speed: u32,                         // Play head Speed
    pub _reserved3: ReservedData<13>,               // RESERVED
    #[deku(endian = "little")]
    pub beat_number: u32,                   // Beat Number
    pub _reserved4: ReservedData<51>,               // RESERVED
    #[deku(endian = "little")]
    pub bpm: u32,                           // BPM
    #[deku(endian = "little")]
    pub pitch_bend: u16,                    // Pitch Bend
    #[deku(endian = "little")]
    pub track_id: u32,                      // Assigned Track ID
}

// DATA PACKET - METADATA (Message Type 200, Data Type 4)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct MetaData {
    pub data_type: u8,                      // Datatype 4 = Metadata
    pub layer_id: u8,                       // Layer ID
    pub _reserved0: ReservedData<1>,        // RESERVED
    pub _reserved1: ReservedData<2>,        // RESERVED
    pub track_artist: [u8; 256],            // Track Artist Name (UTF-16 in v3.5+)
    pub track_title: [u8; 256],             // Track Title Name (UTF-16 in v3.5+)
    #[deku(endian = "little")]
    pub track_key: u16,                     // Track KEY
    #[deku(endian = "little")]
    pub track_id: u32,                      // Assigned Track ID
}

// DATA PACKET - BEAT GRID DATA (Message Type 200, Data Type 8)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct BeatGridHeader {
    data_type: u8,                      // Datatype 8 = Beat Grid Data
    layer_id: u8,                       // Layer Number
    #[deku(endian = "little")]
    data_size: u32,                     // Total Data Size
    #[deku(endian = "little")]
    total_packets: u32,                 // Total Packets used for data
    #[deku(endian = "little")]
    packet_no: u32,                     // Packet Number
    #[deku(endian = "little")]
    data_cluster_size: u32,             // Data Cluster Size
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct BeatGridEntry {
    #[deku(endian = "little")]
    beat_number: u16,                   // Beat Number
    beat_type: u8,                      // 20 = Downbeat, 10 = Upbeat
    _reserved0: ReservedData<1>,        // RESERVED
    #[deku(endian = "little")]
    beat_timestamp: u32,                // Timestamp in MS
}

// DATA PACKET - CUE DATA (Message Type 200, Data Type 12)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct CueEntry {
    cue_type: u8,                       // Cue Type
    _reserved0: ReservedData<1>,        // RESERVED
    #[deku(endian = "little")]
    cue_in_time: u32,                   // CUE IN Time
    #[deku(endian = "little")]
    cue_out_time: u32,                  // CUE OUT Time
    _reserved1: ReservedData<1>,        // RESERVED
    cue_color: [u8; 3],                 // CUE Color (R, G, B)
    _reserved2: ReservedData<8>,        // RESERVED
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct CueData {
    data_type: u8,                      // Datatype 12 = Cue Data
    layer_id: u8,                       // Layer Number
    _reserved0: ReservedData<16>,       // RESERVED
    #[deku(endian = "little")]
    loop_in: u32,                       // Loop IN Time
    #[deku(endian = "little")]
    loop_out: u32,                      // Loop OUT Time
    cues: [CueEntry; 18],               // CUE 1-18
}

// DATA PACKET - SMALL WAVEFORM (Message Type 200, Data Type 16)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct SmallWaveformData {
    data_type: u8,                      // Datatype 16 = Small Waveform
    layer_id: u8,                       // Layer Number
    #[deku(endian = "little")]
    data_size: u32,                     // Total Datasize (2400)
    #[deku(endian = "little")]
    total_packets: u32,                 // Total Packets used for data
    #[deku(endian = "little")]
    packet_no: u32,                     // Packet Number
    _reserved0: ReservedData<4>,                // RESERVED
    waveform_data: [u8; 2400],          // BLevel (Odd Bytes) / BColor (Even Bytes)
}

// DATA PACKET - BIG WAVEFORM (Message Type 200, Data Type 32)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct BigWaveformData {
    data_type: u8,                      // Datatype 32 = Big Waveform
    layer_id: u8,                       // Layer Number
    #[deku(endian = "little")]
    data_size: u32,                     // Total Data size
    #[deku(endian = "little")]
    total_packets: u32,                 // Total Packets used for data
    #[deku(endian = "little")]
    packet_no: u32,                     // Packet Number
    #[deku(endian = "little")]
    data_cluster_size: u32,             // Data Cluster Size (standard: 4800)
    #[deku(count = "data_cluster_size")]
    waveform_data: Vec<u8>,             // BLevel (Odd Bytes) / BColor (Even Bytes)
}

// DATA PACKET - MIXER DATA (Message Type 200, Data Type 150)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct MixerChannel {
    pub source_select: u8,                  // Channel Source Select
    pub audio_level: u8,                    // Channel Audio Level
    pub fader_level: u8,                    // Channel Fader Level
    pub trim_level: u8,                     // Channel Trim Level
    pub comp_level: u8,                     // Channel Compressor Level
    pub eq_hi_level: u8,                    // Channel EQ Hi Level
    pub eq_hi_mid_level: u8,                // Channel EQ Hi Mid Level
    pub eq_low_mid_level: u8,               // Channel EQ Low Mid Level
    pub eq_low_level: u8,                   // Channel EQ Low Level
    pub filter_color: u8,                   // Channel Filter/Color
    pub send: u8,                           // Channel FX Send
    pub cue_a: u8,                          // Channel CUE A
    pub cue_b: u8,                          // Channel CUE B
    pub crossfader_assign: u8,              // Channel Crossfader Assign
    pub _reserved: [u8; 10],                // RESERVED
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct MixerData {
    pub data_type: u8,                      // Datatype 150 = Mixer Data
    pub mixer_id: u8,                       // Mixer ID
    pub mixer_type: u8,                     // Mixer Type
    pub _reserved0: ReservedData<1>,        // RESERVED
    pub _reserved1: ReservedData<1>,        // RESERVED
    pub mixer_name: [u8; 16],               // Name of Mixer
    pub _reserved2: ReservedData<12>,       // RESERVED
    pub _reserved3: ReservedData<2>,        // RESERVED FOR MIC 1-2 LEVEL
    pub mic_eq_hi: u8,                      // Mic EQ HI
    pub mic_eq_low: u8,                     // Mic EQ Low
    pub master_audio_level: u8,             // Master Audio Level
    pub master_fader_level: u8,             // Master Fader Level
    pub _reserved4: ReservedData<4>,        // RESERVED
    pub link_cue_a: u8,                     // Link CUE A
    pub link_cue_b: u8,                     // Link CUE B
    pub master_filter: u8,                  // Master Filter
    pub _reserved5: ReservedData<1>,        // RESERVED
    pub master_cue_a: u8,                   // Master CUE A
    pub master_cue_b: u8,                   // Master CUE B
    pub _reserved6: ReservedData<1>,        // RESERVED
    pub master_isolator_on_off: u8,         // Master Isolator Switch
    pub master_isolator_hi: u8,             // Master Isolator Hi
    pub master_isolator_mid: u8,            // Master Isolator Mid
    pub master_isolator_low: u8,            // Master Isolator Low
    pub _reserved7: ReservedData<1>,        // RESERVED
    pub filter_hpf: u8,                     // Filter HPF
    pub filter_lpf: u8,                     // Filter LPF
    pub filter_resonance: u8,               // Filter Resonance
    pub _reserved8: ReservedData<2>,        // RESERVED
    pub send_fx_effect: u8,                 // Send FX Effect
    pub send_fx_ext_1: u8,                  // Send Return Ext 1
    pub send_fx_ext_2: u8,                  // Send Return Ext 2
    pub send_fx_master_mix: u8,             // Send FX Master Mix
    pub send_fx_size_feedback: u8,          // Send FX Size Feedback
    pub send_fx_time: u8,                   // Send FX Time
    pub send_fx_hpf: u8,                    // Send FX HPF
    pub send_fx_level: u8,                  // Send FX Level
    pub send_return_3_source_select: u8,    // Send Return 3 Source Select
    pub send_return_3_type: u8,             // Send Return 3 Type
    pub send_return_3_on_off: u8,           // Send Return 3 ON/OFF
    pub send_return_3_level: u8,            // Send Return 3 Level
    pub _reserved9: ReservedData<1>,        // RESERVED
    pub channel_fader_curve: u8,            // Channel Fader Curve
    pub cross_fader_curve: u8,              // Cross Fader Curve
    pub cross_fader: u8,                    // Cross Fader
    pub beat_fx_on_off: u8,                 // BeatFX ON/OFF
    pub beat_fx_level_depth: u8,            // BeatFX Level/Depth
    pub beat_fx_channel_select: u8,         // BeatFX Channel Select
    pub beat_fx_select: u8,                 // BeatFX Select
    pub beat_fx_freq_hi: u8,                // BeatFX Frequency Hi
    pub beat_fx_freq_mid: u8,               // BeatFX Frequency Mid
    pub beat_fx_freq_low: u8,               // BeatFX Frequency Low
    pub headphones_pre_eq: u8,              // Headphones Pre EQ
    pub headphones_a_level: u8,             // Headphones A Level
    pub headphones_a_mix: u8,               // Headphones A Mix
    pub headphones_b_level: u8,             // Headphones B Level
    pub headphones_b_mix: u8,               // Headphones B Mix
    pub booth_level: u8,                    // Booth Level
    pub booth_eq_hi: u8,                    // Booth EQ Hi
    pub booth_eq_low: u8,                   // Booth EQ Low
    pub _reserved10: [u8; 10],              // RESERVED
    pub channels: [MixerChannel; 6],        // Channels 1-6
}

// FILE PACKET - LOW RES ARTWORK (Message Type 204, Data Type 128)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct ArtworkFileData {
    data_type: u8,                      // Datatype 128 = Low Res Artwork File
    layer_id: u8,                       // Layer Number
    #[deku(endian = "little")]
    data_size: u32,                     // Total Data size
    #[deku(endian = "little")]
    total_packets: u32,                 // Total Packets used for data
    #[deku(endian = "little")]
    packet_no: u32,                     // Packet Number
    #[deku(endian = "little")]
    data_cluster_size: u32,             // Data Cluster Size (standard: 4800)
    #[deku(count = "data_cluster_size")]
    file_data: Vec<u8>,                 // Raw JPEG file data
}

// APPLICATION SPECIFIC DATA PACKET (Message Type 30 / 213)
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct AppSpecificData {
    data_identifier_1: u8,              // Application Identifier Signature 1/2
    data_identifier_2: u8,              // Application Identifier Signature 2/2
    #[deku(endian = "little")]
    data_size: u32,                     // Data Size of all packets
    #[deku(endian = "little")]
    total_packets: u32,                 // Total of all packets
    #[deku(endian = "little")]
    packet_no: u32,                     // Packet No
    #[deku(endian = "little")]
    packet_signature: u32,              // Signature of Packet (178260640)
    #[deku(count = "data_size")]
    data: Vec<u8>,                      // Data
}

// TIME PACKET (Message Type 254)
#[derive(Debug, Clone, PartialEq, DekuRead, DekuWrite)]
pub struct LayerTimecode {
    pub smpte_mode: u8,                     // Layer SMPTE Mode (24/25/29/30)
    pub state: u8,                          // Time Code State (0=Stopped, 1=Running, 2=Force Resync)
    pub hours: u8,                          // Time Code Hours (0-23)
    pub minutes: u8,                        // Time Code Minutes (0-59)
    pub seconds: u8,                        // Time Code Seconds (0-59)
    pub frames: u8,                         // Time Code Frames
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
pub struct TimePacketData {
    #[deku(endian = "little")]
    pub l1_time: u32,                       // LAYER 1 Current Time in Milliseconds
    #[deku(endian = "little")]
    pub l2_time: u32,                       // LAYER 2 Current Time in Milliseconds
    #[deku(endian = "little")]
    pub l3_time: u32,                       // LAYER 3 Current Time in Milliseconds
    #[deku(endian = "little")]
    pub l4_time: u32,                       // LAYER 4 Current Time in Milliseconds
    #[deku(endian = "little")]
    pub la_time: u32,                       // LAYER A Current Time in Milliseconds
    #[deku(endian = "little")]
    pub lb_time: u32,                       // LAYER B Current Time in Milliseconds
    #[deku(endian = "little")]
    pub lm_time: u32,                       // LAYER M Current Time in Milliseconds
    #[deku(endian = "little")]
    pub lc_time: u32,                       // LAYER C Current Time in Milliseconds
    #[deku(endian = "little")]
    pub l1_total_time: u32,                 // LAYER 1 Total Time in Milliseconds
    #[deku(endian = "little")]
    pub l2_total_time: u32,                 // LAYER 2 Total Time in Milliseconds
    #[deku(endian = "little")]
    pub l3_total_time: u32,                 // LAYER 3 Total Time in Milliseconds
    #[deku(endian = "little")]
    pub l4_total_time: u32,                 // LAYER 4 Total Time in Milliseconds
    #[deku(endian = "little")]
    pub la_total_time: u32,                 // LAYER A Total Time in Milliseconds
    #[deku(endian = "little")]
    pub lb_total_time: u32,                 // LAYER B Total Time in Milliseconds
    #[deku(endian = "little")]
    pub lm_total_time: u32,                 // LAYER M Total Time in Milliseconds
    #[deku(endian = "little")]
    pub lc_total_time: u32,                 // LAYER C Total Time in Milliseconds
    pub l1_beat_marker: u8,                 // Layer 1 Beatmarker
    pub l2_beat_marker: u8,                 // Layer 2 Beatmarker
    pub l3_beat_marker: u8,                 // Layer 3 Beatmarker
    pub l4_beat_marker: u8,                 // Layer 4 Beatmarker
    pub la_beat_marker: u8,                 // Layer A Beatmarker
    pub lb_beat_marker: u8,                 // Layer B Beatmarker
    pub lm_beat_marker: u8,                 // Layer M Beatmarker
    pub lc_beat_marker: u8,                 // Layer C Beatmarker
    pub l1_layer_state: u8,                 // Layer 1 Layer State
    pub l2_layer_state: u8,                 // Layer 2 Layer State
    pub l3_layer_state: u8,                 // Layer 3 Layer State
    pub l4_layer_state: u8,                 // Layer 4 Layer State
    pub la_layer_state: u8,                 // Layer A State
    pub lb_layer_state: u8,                 // Layer B State
    pub lm_layer_state: u8,                 // Layer M State
    pub lc_layer_state: u8,                 // Layer C State
    pub _reserved0: ReservedData<1>,                     // RESERVED
    pub smpte_mode: u8,                     // General SMPTE Mode
    pub l1_timecode: LayerTimecode,         // Layer 1 Timecode
    pub l2_timecode: LayerTimecode,         // Layer 2 Timecode
    pub l3_timecode: LayerTimecode,         // Layer 3 Timecode
    pub l4_timecode: LayerTimecode,         // Layer 4 Timecode
    pub la_timecode: LayerTimecode,         // Layer A Timecode
    pub lb_timecode: LayerTimecode,         // Layer B Timecode
    pub lm_timecode: LayerTimecode,         // Layer M Timecode
    pub lc_timecode: LayerTimecode,         // Layer C Timecode
    pub l1_on_air: u8,                      // Layer 1 OnAir State (fader position 0-255)
    pub l2_on_air: u8,                      // Layer 2 OnAir State
    pub l3_on_air: u8,                      // Layer 3 OnAir State
    pub l4_on_air: u8,                      // Layer 4 OnAir State
    pub la_on_air: u8,                      // Layer A OnAir State
    pub lb_on_air: u8,                      // Layer B OnAir State
    pub lm_on_air: u8,                      // Layer M OnAir State
    pub lc_on_air: u8,                      // Layer C OnAir State
}

#[derive(Debug, Clone)]
pub enum Data {
    OptIn(OptInData),
    OptOut(OptOutData),
    Status(StatusData),
    TimeSync(TimeSyncData),
    ErrorNotification(ErrorNotificationData),
    Request(RequestData),
    AppSpecific(AppSpecificData),
    Control(ControlData),
    Text(TextData),
    Keyboard(KeyboardData),
    Metrics(MetricsData),
    Meta(MetaData),
    BeatGrid(BeatGridHeader),
    Cue(CueData),
    SmallWaveform(SmallWaveformData),
    BigWaveform(BigWaveformData),
    Mixer(MixerData),
    ArtworkFile(ArtworkFileData),
    Time(TimePacketData),
}