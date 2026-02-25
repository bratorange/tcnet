use deku::{DekuRead, DekuWrite};

pub type NodeId = u16;
pub type NodeOptions = u16;
pub type Timestamp = u32; // timestamp in microseconds

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(id_type = "u8")]
pub enum NodeType {
    #[deku(id = 0)]
    Default = 1,
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
pub struct ManagementHeader {
    node_id: NodeId,
    protocol_version_major: u8,
    protocol_version_minor: u8,
    header: [u8; 3], // this just here for serde purposes and must allways be "TCN"
    message_type: u8,
    mode_name: [u8; 8],
    seq: u8,
    node_type: NodeType,
    node_options: NodeOptions,
    timestamp: Timestamp,
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
struct NodeConfig {
    need_authentication: bool,
    supports_tcncm: bool,
    supports_tcnasdps: bool,
    dnd: bool,
}