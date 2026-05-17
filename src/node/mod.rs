use crate::into_ascii;
use crate::protocol::{AsciiString, NodeId, NodeOptions, NodeType};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};
pub(crate) mod dj_controller;
pub(crate) mod tcnet_packet;
pub(crate) mod dispatcher;
pub(crate) mod response_data;

use crate::node::dj_controller::DjController;

#[derive(Debug)]
pub(crate) struct ForeignNode {
    pub last_seen: u64,
    /// Full socket address including the listener port (from OptIn).
    pub address: SocketAddrV4,
    pub config: ApplicationConfig,
    pub dj_controller: Option<DjController>,
}

#[derive(Default)]
pub(crate) struct DynamicNodeState {
    pub discovered_nodes: HashMap<SocketAddrV4, ForeignNode>,
    pub uptime: u16,
    pub current_seq: u8,
}

/// Identity and bind configuration used by a [`TCNetClient`](crate::TCNetClient)
/// when it announces itself to peers.
///
/// The defaults bind to `0.0.0.0:65023` and identify as a `Slave` node with
/// neutral vendor / application names — enough to be discovered on a TCNet
/// network without further setup. Override any field as needed:
///
/// ```
/// use std::net::{Ipv4Addr, SocketAddrV4};
/// use tcnet::{ApplicationConfig, NodeType};
/// use tcnet::into_ascii;
///
/// let mut config = ApplicationConfig::default();
/// config.node_id = 0x42;
/// config.node_type = NodeType::Slave;
/// config.application_name = into_ascii!("myviz__________");
/// config.address = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 65023);
/// ```
///
/// Note that `address.port()` is the **preferred** unicast port — the dispatcher
/// also binds the spec-mandated broadcast ports `60000`, `60001` and `60002`
/// regardless of this value, and falls back to the next free port if the
/// preferred one is taken.
#[derive(Debug, Clone, Copy)]
pub struct ApplicationConfig {
    /// 16-bit node identifier sent in every outgoing `ManagementHeader`. Choose
    /// any value not already in use on the network.
    pub node_id: NodeId,
    /// Whether this node acts as `Master`, `Slave`, `Auto` or `Repeater`. See
    /// [`NodeType`](crate::NodeType).
    pub node_type: NodeType,
    /// Vendor identifier shown to peers in the OptIn packet.
    pub vendor_name: AsciiString<16>,
    /// Application / device identifier shown to peers in the OptIn packet.
    pub application_name: AsciiString<16>,
    /// Application major version (semver-ish — emitted in OptIn).
    pub application_major_version: u8,
    /// Application minor version.
    pub application_minor_version: u8,
    /// Application patch / bug-fix version.
    pub application_bug_version: u8,
    /// Short human-readable name for this node (shown to peers in the header).
    pub node_name: AsciiString<8>,
    /// Capability bitflags (authentication, control-message support, …).
    /// See [`NodeOptions`](crate::NodeOptions).
    pub node_options: NodeOptions,
    /// Bind address. The IP is the local interface to listen on; the port is
    /// the *preferred* unicast port (broadcasts always use 60000 / 60001 / 60002).
    pub address: SocketAddrV4,
}

impl Default for ApplicationConfig {
    fn default() -> Self {
        Self {
            node_id: 0,
            node_type: NodeType::Slave,
            vendor_name: into_ascii!("Somevendor______"),
            application_name: into_ascii!("Someapplication_"),
            application_major_version: 0,
            application_minor_version: 0,
            application_bug_version: 102,
            node_name: into_ascii!("Default_"),
            node_options: NodeOptions::empty(),
            address: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 65_023),
        }
    }
}
