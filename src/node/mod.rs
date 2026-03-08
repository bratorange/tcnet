use crate::into_ascii;
use crate::node::tcnet_packet::{opt_in_node_config, Packet};
use crate::node::tcnet_packet_serde::Data::{OptIn, OptOut};
use crate::node::tcnet_packet_serde::{AsciiString, Data, ManagementHeader, NodeId, NodeOptions, NodeType, OptInData};
use deku::{DekuContainerWrite, DekuError};
use log::{error, info, trace, warn};
use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use getifs::best_local_ipv4_addrs;
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio::sync::RwLock;
use tokio::time::interval;
use crate::application::ApplicationNode;

pub(crate) mod tcnet_packet_serde;
pub(crate) mod tcnet_packet;

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
pub(crate) struct ApplicationConfig {
    pub node_id: NodeId,
    pub node_type: NodeType,
    pub vendor_name: AsciiString<16>,
    pub application_name: AsciiString<16>,
    pub application_major_version: u8,
    pub application_minor_version: u8,
    pub application_bug_version: u8,
    pub node_name: AsciiString<8>,
    pub node_options: NodeOptions,
    pub unicast_port: u16, // only used for foreign nodes
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

#[derive(Clone)]
pub struct Dispatcher {
    pub(crate) application_nodes: Arc<RwLock<HashMap<NodeId, ApplicationNode>>>,
    pub(crate) bind_address: Ipv4Addr,
    pub unicast_port: u16,
    pub(crate) state: Arc<RwLock<DynamicNodeState>>,
}

pub fn send_message(application: &ApplicationNode, address: Ipv4Addr, node_id: NodeId, data: Data){
    let dispatcher = &application.dispatcher;
    todo!()
}

pub async fn start_node(dispatcher: Arc<Dispatcher>) {
    trace!("binding sockets...");
    let broadcast_socket_addr = SocketAddr::new(dispatcher.bind_address.into(), 60_000);
    let broadcast_socket = Arc::new(UdpSocket::bind(broadcast_socket_addr).await
        .expect("Could not bind to socket 60000"));
    broadcast_socket.set_broadcast(true)
        .expect("Could not enable socket 60000 for broadcasting");

    let time_broadcast_addr = SocketAddr::new(dispatcher.bind_address.into(), 60_001);
    let time_broadcast_socket = Arc::new(UdpSocket::bind(time_broadcast_addr).await
        .expect("Could not bind to socket 60001"));
    time_broadcast_socket.set_broadcast(true)
        .expect("Could not enable socket 60001 for broadcasting");

    // The spec requires also listening on this port. However, there is no special use case
    // declared for it.
    let broadcast_socket_addr2 = SocketAddr::new(dispatcher.bind_address.into(), 60_002);
    let broadcast_socket2 = Arc::new(UdpSocket::bind(broadcast_socket_addr2).await
        .expect("Could not bind to broadcast socket 60002"));
    broadcast_socket2.set_broadcast(true)
        .expect("Could not enable socket 60002 for broadcasting");

    let unicast_socket_addr = SocketAddr::new(dispatcher.bind_address.into(), dispatcher.unicast_port);
    let unicast_socket = Arc::new(UdpSocket::bind(unicast_socket_addr).await
        .expect("Could not bind to unicast socket")
    );

    trace!("Starting network processing");

    spawn(broadcast(dispatcher.clone(), broadcast_socket.clone()));
    spawn(listen(dispatcher.clone(), broadcast_socket.clone()));
    spawn(listen(dispatcher.clone(), broadcast_socket2.clone()));
    spawn(listen(dispatcher.clone(), time_broadcast_socket.clone()));
    spawn(listen(dispatcher.clone(), unicast_socket.clone()));
    spawn(timeout_foreign_nodes(dispatcher.clone()));
}

async fn listen(dispatcher: Arc<Dispatcher>, socket: Arc<UdpSocket>) -> io::Result<()> {
    let mut buffer = [0; 1024];
    info!("Start Listening for packets on {}", socket.local_addr()?);
    loop {
        match socket.recv_from(&mut buffer).await {
            Ok((size, src)) => {
                trace!("Received {} bytes from {}", size, src);
                match Packet::deserialize_packet(&buffer) {
                    Ok(packet) => {
                        trace!("Received packet:");
                        trace!("{:?}", packet);

                        let src_addr = match src {
                            SocketAddr::V4(addr) => addr.ip().clone(),
                            _ => unreachable!(),
                        };
                        match &packet.data {
                            OptIn(opt_in_data) => {
                                let node_config = opt_in_node_config(&packet.header, opt_in_data);

                                let mut state = dispatcher.state.write().await;
                                let outer = state.discovered_nodes.entry(
                                    src_addr,
                                ).or_insert(ForeignNode { last_seen: 0, address: src_addr, applications: HashMap::new(), });
                                outer.last_seen = timestamp_secs();
                                let inner = outer.applications.entry(node_config.node_id).or_insert(node_config);
                                *inner = node_config;
                            }

                            OptOut(_) => {
                                let removed_node =
                                    dispatcher.state.write().await.discovered_nodes.remove(&src_addr);
                                if removed_node.is_some() {
                                    warn!(
                                    "Node {} has opted out despite not being part of the network (anymore)",
                                    src_addr);
                                } else {
                                    info!("Node {} has opted out of the network", src_addr);
                                }
                            }
                            _ => {},
                        };

                        // send data to its respective application
                        // TODO use channels for communication with applications
                        if let Some(application) =
                            dispatcher.application_nodes.write().await.get_mut(&packet.header.node_id) {
                            application.handle_incoming_message(&packet.header, &packet.data)
                        }
                    },
                    Err(e) => {
                        error!("{:?}", e);
                    },
                }
            },
            Err(e) => {
                error!("Network error: {}", e);
            },
        };
    }
}

async fn broadcast(dispatcher: Arc<Dispatcher>, broadcast_socket: Arc<UdpSocket>) {
    // TCNet spec requires opt in broadcast messages once per second
    // TODO only send to the broadcast address of bind addr
    let ipv4_addrs = best_local_ipv4_addrs()
        .expect("Could not get local IPv4 addresses")
        .iter().map(|net| SocketAddr::V4(SocketAddrV4::new(net.broadcast(), 60_000)))
        .collect::<HashSet<_>>();
    trace!("Broadcasting opt in packets to {:?}", ipv4_addrs);
    let mut interval = interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        trace!("Sending opt in packets...");

        let mut dispatcher_state = dispatcher.state.write().await;
        let seq = dispatcher_state.current_seq;

        for node in dispatcher.application_nodes.read().await.values() {
            let config = node.config;
            let payload = opt_in_packet(&config, &dispatcher_state, seq)
                .expect("TCNet: Could not serialize opt in packet");
            for addr in &ipv4_addrs {
                let _ = broadcast_socket.send_to(&payload, addr).await;
                dispatcher_state.current_seq += 1;
            }
        }

        // TODO unicast opt in
        trace!("Sent opt in packet");
    }
}

async fn timeout_foreign_nodes(node: Arc<Dispatcher>){
    let mut interval = interval(Duration::from_secs(1));
    loop {
        let secs = timestamp_secs();
        node.state.write().await.discovered_nodes.retain(|_, foreign_node| {
            let keep = foreign_node.last_seen + 10 > secs;
            if !keep {
                warn!("Node {} timed out", foreign_node.address);
            }
            keep
        });
        interval.tick().await;
    }
}

pub fn timestamp_micros() -> u32 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("time should go forward");
    // tcnet's clock expects to be reset every second
    since_the_epoch.subsec_micros()
}

pub fn timestamp_secs() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("time should go forward");
    since_the_epoch.as_secs()
}

fn management_header(app_config: &ApplicationConfig, message_type: u8, seq: u8) -> ManagementHeader{
    ManagementHeader{
        node_id: app_config.node_id,
        protocol_version_major: 3,
        protocol_version_minor: 6,
        _header: into_ascii!("TCN"),
        message_type,
        node_name: app_config.node_name,
        seq,
        node_type: app_config.node_type,
        node_options: app_config.node_options,
        timestamp: timestamp_micros(),
    }
}
fn opt_in_packet(app_config: &ApplicationConfig, node_state: &DynamicNodeState, seq: u8) -> Result<Vec<u8>, DekuError> {
    let header = management_header(app_config, 2, seq);
    let data = OptInData{
        node_count: node_state.discovered_nodes.len() as u16,
        node_listener_port: app_config.unicast_port,
        uptime: node_state.uptime,
        _reserved0: Default::default(),
        vendor_name: app_config.vendor_name,
        application: app_config.application_name,
        application_major_version: app_config.application_major_version,
        application_minor_version: app_config.application_minor_version,
        application_bug_version: app_config.application_bug_version,
        _reserved1: Default::default(),
    };
    let ret = [header.to_bytes()?, data.to_bytes()?].concat();
    debug_assert!(ret.len() == 68);
    Ok(ret)
}
