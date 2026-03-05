use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use deku::{DekuContainerWrite, DekuError, DekuRead, DekuWrite};
use log::{error, trace};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::RwLock;
use tokio::time::interval;
use crate::into_ascii;
use crate::tcnet_packet::Packet;
use crate::tcnet_packet_serde::{AsciiString, ManagementHeader, NodeId, NodeOptions, NodeType, OptInData};

#[derive(Debug, Default)]
pub(crate) struct DynamicNodeState {
    pub discovered_nodes: Vec<NodeConfig>,
    pub uptime: u16,
    pub timestamp: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct NodeConfig {
    pub node_id: NodeId,
    pub node_type: NodeType,
    pub address: Ipv4Addr,
    pub unicast_port: u16,
    pub vendor_name: AsciiString<16>,
    pub application_name: AsciiString<16>,
    pub application_major_version: u8,
    pub application_minor_version: u8,
    pub application_bug_version: u8,
    pub mode_name: AsciiString<8>,
    pub node_options: NodeOptions,
}

// TODO dont use Default here
impl Default for NodeConfig{
    fn default() -> Self {
        Self{
            node_id: 0,
            node_type: NodeType::Auto,
            address: Ipv4Addr::new(0, 0, 0, 0),
            unicast_port: 65_023,
            vendor_name: into_ascii!("Somevendor______"),
            application_name: into_ascii!("Someapplication_"),
            application_major_version: 0,
            application_minor_version: 0,
            application_bug_version: 102,
            mode_name: into_ascii!("Default_"),
            node_options: NodeOptions::empty(),
        }
    }
}

#[derive(Clone)]
pub struct Node {
    pub(crate) config: NodeConfig,
    pub(crate) state: Arc<RwLock<DynamicNodeState>>,
}

impl Node {
    pub fn run(bind_address: Ipv4Addr) -> io::Result<(Self, Runtime)> {
        // create prototypical node for test purposes
        let mut node = Self {
            config: NodeConfig::default(),
            state: Arc::new(RwLock::new(DynamicNodeState::default())),
        };
        node.config.address = bind_address;

        let broadcast_socket_addr = SocketAddr::new(node.config.address.into(), 60_000);
        let broadcast_socket = Arc::new(UdpSocket::bind(broadcast_socket_addr)?);
        broadcast_socket.set_broadcast(true)?;

        let time_broadcast_addr = SocketAddr::new(node.config.address.into(), 60_001);
        let time_broadcast_socket = Arc::new(UdpSocket::bind(time_broadcast_addr)?);
        time_broadcast_socket.set_broadcast(true)?;

        // The spec requires also listening on this port. However, there is no special use case
        // declared for it.
        let broadcast_socket_addr2 = SocketAddr::new(node.config.address.into(), 60_002);
        let broadcast_socket2 = Arc::new(UdpSocket::bind(broadcast_socket_addr2)?);
        broadcast_socket2.set_broadcast(true)?;

        let unicast_socket_addr = SocketAddr::new(node.config.address.into(), node.config.unicast_port);
        let unicast_socket = Arc::new(UdpSocket::bind(unicast_socket_addr)?);


        let rt = Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("tcnet")
            .enable_all()
            .build()?;

        rt.spawn(listen(node.clone(), broadcast_socket.clone()));
        rt.spawn(listen(node.clone(), broadcast_socket2.clone()));
        rt.spawn(listen(node.clone(), time_broadcast_socket.clone()));
        rt.spawn(listen(node.clone(), unicast_socket.clone()));
        rt.block_on(broadcast(node.clone(), broadcast_socket.clone()));
        Ok((node, rt))
    }
}

async fn listen(node: Node, socket: Arc<UdpSocket>) -> io::Result<()> {
    loop {
        let mut buffer = [0; 1024];
        match socket.recv_from(&mut buffer) {
            Ok((size, src)) => {
                trace!("Received {} bytes from {}", size, src);
            },
            Err(e) => {
                error!("Network error: {}", e);
            },
        };

        match Packet::deserialize_packet(&buffer) {
            Ok(packet) => {
                trace!("Received packet:");
                trace!("{:?}", packet);
            },
            Err(e) => {
                error!("{:?}", e);
            },
        }
    }
}
async fn broadcast(node: Node, broadcast_socket: Arc<UdpSocket>) {
    // TCNet spec requires opt in broadcast messages once per second
    let broadcast_addr = SocketAddr::V4(SocketAddrV4::new([255, 255, 255, 255].into(), 60_000));
    let mut interval = interval(Duration::from_secs(1));
    loop {
        let node_state = node.state.read().await;
        let payload = opt_in_packet(&node, &node_state, 0)
            .expect("TCNet: Could not serialize opt in packet");
        let _ = broadcast_socket.send_to(&payload, broadcast_addr);
        trace!("Sent opt in packet");
        interval.tick().await;
    }
}

pub fn timestamp() -> u32 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("time should go forward");
    // tcnet's clock expects to be reset every second
    since_the_epoch.subsec_micros()
}

fn management_header(node: &Node, message_type: u8, seq: u8) -> ManagementHeader{
    ManagementHeader{
        node_id: node.config.node_id,
        protocol_version_major: 3,
        protocol_version_minor: 6,
        _header: into_ascii!("TCN"),
        message_type,
        mode_name: node.config.mode_name,
        seq,
        node_type: node.config.node_type as u8,
        node_options: node.config.node_options,
        timestamp: timestamp(),
    }
}
fn opt_in_packet(node: &Node, node_state: &DynamicNodeState, seq: u8) -> Result<Vec<u8>, DekuError> {
    let header = management_header(node, 2, seq);
    let data = OptInData{
        node_count: node_state.discovered_nodes.len() as u16,
        node_listener_port: node.config.unicast_port,
        uptime: node_state.uptime,
        _reserved0: Default::default(),
        vendor_name: node.config.vendor_name,
        application: node.config.application_name,
        application_major_version: node.config.application_major_version,
        application_minor_version: node.config.application_minor_version,
        application_bug_version: node.config.application_bug_version,
        _reserved1: Default::default(),
    };
    let ret = [header.to_bytes()?, data.to_bytes()?].concat();
    debug_assert!(ret.len() == 68);
    Ok(ret)
}