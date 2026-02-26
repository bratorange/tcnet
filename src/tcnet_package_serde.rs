use std::fmt::Debug;
use deku::{DekuRead, DekuWrite};

pub type NodeId = u16;
pub type NodeOptions = u16;
pub type Timestamp = u32; // timestamp in microseconds

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum NodeType {
    Variant = 0, // TODO
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum AutoMasterMode{
    Variant = 0, // TODO
}

#[derive(PartialEq, DekuRead, DekuWrite)]
pub struct AsciiString<const N: usize>(pub [u8; N]);

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

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
pub struct ManagementHeader {
    pub node_id: NodeId,
    pub protocol_version_major: u8,
    pub protocol_version_minor: u8,
    pub _header: [u8; 3], // this just here for serde purposes and must allways be "TCN"
    pub message_type: u8,
    pub mode_name: AsciiString<8>,
    pub seq: u8,
    pub node_type: u8,
    pub node_options: NodeOptions,
    pub timestamp: Timestamp,
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
pub struct OptInData{
    node_count: u16, // Amount of Registered Node
    node_listener_port: u16,            // Listener Port for Unicast Messages
    uptime: u16,                        // Uptime of Node in SEC
    _reserved0: [u8; 2],                // RESERVED
    vendor_name: [u8; 16],              // Vendor
    application: [u8; 16],              // Application / Device Name
    application_major_version: u8,      // Application/Device Major Version
    application_minor_version: u8,      // Application/Device Minor Version
    application_bug_version: u8,        // Application/Device Minor Version
    _reserved1: [u8; 1],                // RESERVED
}

// TODO Opt-Out package

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum LayerStatus{
    Variant = 0, // TODO
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
pub struct StatusData {
    node_count: u16,                    // Amount of Registered Nodes
    node_listener_port: u16,            // Listener Port for Unicast Messages
    _reserved0:[u8; 6],                 // RESERVED
    layer_1_source: u8,                 // Layer 1 Source
    layer_2_source: u8,                 // Layer 2 Source
    layer_3_source: u8,                 // Layer 3 Source
    layer_4_source: u8,                 // Layer 4 Source
    layer_a_source: u8,                 // Layer A Source
    layer_b_source: u8,                 // Layer B Source
    layer_m_source: u8,                 // Layer M Source
    layer_c_source: u8,                 // Layer C Source
    layer_1_status: LayerStatus,        // Layer 1 Status
    layer_2_status: LayerStatus,        // Layer 2 Status
    layer_3_status: LayerStatus,        // Layer 3 Status
    layer_4_status: LayerStatus,        // Layer 4 Status
    layer_a_status: LayerStatus,        // Layer A Status
    layer_b_status: LayerStatus,        // Layer B Status
    layer_m_status: LayerStatus,        // Layer M Status
    layer_c_status: LayerStatus,        // Layer C Status
    layer_1_track_id: u32,              // Assigned Track ID for Layer 1
    layer_2_track_id: u32,              // Assigned Track ID for Layer 2
    layer_3_track_id: u32,              // Assigned Track ID for Layer 3
    layer_4_track_id: u32,              // Assigned Track ID for Layer 4
    layer_a_track_id: u32,              // Assigned Track ID for Layer A
    layer_b_track_id: u32,              // Assigned Track ID for Layer B
    layer_m_track_id: u32,              // Assigned Track ID for Layer M
    layer_c_track_id: u32,              // Assigned Track ID for Layer C
    _reserved1: [u8; 1],                // RESERVED
    smpte_mode: u8,              // SMPTE Mode
    auto_master_mode: AutoMasterMode,   // Auto Master Mode
    _reserved2: [u8; 15],               // RESERVED
    app_specific: [u8; 72],             // APP SPECIFIC
    layer_1_name: [u8; 16],             // Layer 1 Source
    layer_2_name: [u8; 16],             // Layer 2 Source
    layer_3_name: [u8; 16],             // Layer 3 Source
    layer_4_name: [u8; 16],             // Layer 4 Source
    layer_a_name: [u8; 16],             // Layer A Source
    layer_b_name: [u8; 16],             // Layer B Source
    layer_m_name: [u8; 16],             // Layer M Source
    layer_c_name: [u8; 16],             // Layer C Source
}

#[derive(Debug)]
pub enum Data{
    OptIn(OptInData),
    Status(StatusData),
}