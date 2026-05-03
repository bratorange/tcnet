use crate::into_ascii;
use crate::node::tcnet_packet_serde::{AsciiString, NodeId, NodeOptions, NodeType};
use std::collections::HashMap;
use std::net::Ipv4Addr;
pub(crate) mod tcnet_packet_serde;
pub(crate) mod tcnet_packet;
pub mod dispatcher;

#[derive(Clone)]
pub(crate) struct ForeignNode {
    pub last_seen: u64,
    pub address: Ipv4Addr,
    pub applications: HashMap<NodeId, ApplicationConfig>
}

#[derive(Default)]
pub(crate) struct DynamicNodeState {
    pub discovered_nodes: HashMap<Ipv4Addr, ForeignNode>,
    pub uptime: u16,
    pub current_seq: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct ApplicationConfig {
    pub node_id: NodeId,
    pub node_type: NodeType,
    pub vendor_name: AsciiString<16>,
    pub application_name: AsciiString<16>,
    pub application_major_version: u8,
    pub application_minor_version: u8,
    pub application_bug_version: u8,
    pub node_name: AsciiString<8>,
    pub node_options: NodeOptions,
    pub unicast_port: u16, // TODO: only used for foreign nodes, make this more coherent
}

// TODO dont use Default here
impl Default for ApplicationConfig {
    fn default() -> Self {
        Self {
            node_id: 0,
            node_type: NodeType::Auto,
            vendor_name: into_ascii!("Somevendor______"),
            application_name: into_ascii!("Someapplication_"),
            application_major_version: 0,
            application_minor_version: 0,
            application_bug_version: 102,
            node_name: into_ascii!("Default_"),
            node_options: NodeOptions::empty(),
            unicast_port: 65_023,
        }
    }
}

