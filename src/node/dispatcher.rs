use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use deku::DekuContainerWrite;
use tokio::net::UdpSocket;
use log::{error, info, trace, warn};
use tokio::time::interval;
use tokio::spawn;
use tokio::sync::RwLock;
use getifs::best_local_ipv4_addrs;
use crate::node::{ApplicationConfig, DynamicNodeState, ForeignNode};
use crate::node::dj_controller::{DjController, OutgoingRequest};
use crate::node::tcnet_packet::{management_header, opt_in_node_config, opt_in_packet, Packet};
use crate::node::tcnet_packet::Data::{OptIn, OptOut};
use crate::node::tcnet_packet::Data;
use crate::ForeignNodeInfo;

pub struct Dispatcher {
    pub(crate) node_config: ApplicationConfig,
    pub(crate) bind_address: Ipv4Addr,
    pub unicast_port: u16,
    pub(crate) state: Arc<RwLock<DynamicNodeState>>,
    pub(crate) outgoing_tx: kanal::Sender<OutgoingRequest>,
    pub(crate) outgoing_rx: kanal::Receiver<OutgoingRequest>,
    pub(crate) nodes_buf_input: Arc<Mutex<triple_buffer::Input<Vec<ForeignNodeInfo>>>>,
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
    spawn(send(dispatcher.clone(), unicast_socket.clone()));
    spawn(timeout_foreign_nodes(dispatcher.clone()));
}

fn is_dj_packet(data: &Data) -> bool {
    matches!(data,
        Data::Status(_) | Data::Metrics(_) | Data::BeatGrid(_) | Data::Cue(_)
        | Data::SmallWaveform(_) | Data::BigWaveform(_) | Data::Mixer(_)
        | Data::ArtworkFile(_) | Data::Time(_)
    )
}

fn publish_nodes_snapshot(
    state: &DynamicNodeState,
    nodes_input: &Arc<Mutex<triple_buffer::Input<Vec<ForeignNodeInfo>>>>,
) {
    let snapshot: Vec<ForeignNodeInfo> = state.discovered_nodes.values()
        .map(ForeignNodeInfo::from)
        .collect();
    nodes_input.lock().unwrap().write(snapshot);
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
                        trace!("Received packet: {:?}", packet);

                        let src_addr = match src {
                            SocketAddr::V4(addr) => *addr.ip(),
                            _ => unreachable!(),
                        };

                        match &packet.data {
                            OptIn(opt_in_data) => {
                                let node_config = opt_in_node_config(&packet.header, opt_in_data);
                                let mut state = dispatcher.state.write().await;
                                {
                                    let outer = state.discovered_nodes.entry(src_addr)
                                        .or_insert(ForeignNode {
                                            last_seen: 0,
                                            address: src_addr,
                                            applications: HashMap::new(),
                                            dj_controller: None,
                                        });
                                    outer.last_seen = timestamp_secs();
                                    let inner = outer.applications.entry(node_config.node_id).or_insert(node_config);
                                    *inner = node_config;
                                }
                                publish_nodes_snapshot(&state, &dispatcher.nodes_buf_input);
                            }

                            OptOut(_) => {
                                let mut state = dispatcher.state.write().await;
                                let removed = state.discovered_nodes.remove(&src_addr);
                                if removed.is_none() {
                                    warn!("Node {} has opted out despite not being part of the network (anymore)", src_addr);
                                } else {
                                    info!("Node {} has opted out of the network", src_addr);
                                }
                                publish_nodes_snapshot(&state, &dispatcher.nodes_buf_input);
                            }

                            _ => {}
                        }

                        if is_dj_packet(&packet.data) {
                            let mut state = dispatcher.state.write().await;
                            let created_new_ctrl = {
                                let foreign_node = state.discovered_nodes
                                    .entry(src_addr)
                                    .or_insert(ForeignNode {
                                        last_seen: timestamp_secs(),
                                        address: src_addr,
                                        applications: HashMap::new(),
                                        dj_controller: None,
                                    });
                                foreign_node.last_seen = timestamp_secs();

                                let mut new_ctrl = false;
                                if foreign_node.dj_controller.is_none() {
                                    let port = foreign_node.applications.values()
                                        .next()
                                        .map(|a| a.unicast_port)
                                        .unwrap_or(65_023);
                                    let (ctrl, task_fut) = DjController::new(
                                        dispatcher.outgoing_tx.clone(),
                                        src_addr,
                                        port,
                                    );
                                    foreign_node.dj_controller = Some(ctrl);
                                    spawn(task_fut);
                                    new_ctrl = true;
                                }

                                if let Some(ctrl) = &foreign_node.dj_controller {
                                    let _ = ctrl.packet_tx.try_send(packet);
                                }
                                new_ctrl
                            };

                            if created_new_ctrl {
                                publish_nodes_snapshot(&state, &dispatcher.nodes_buf_input);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Incoming packet deserialization failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                error!("Network error: {}", e);
            }
        }
    }
}

async fn send(dispatcher: Arc<Dispatcher>, socket: Arc<UdpSocket>) {
    loop {
        let mut msgs: Vec<OutgoingRequest> = Vec::new();
        if dispatcher.outgoing_rx.drain_into(&mut msgs).is_err() {
            break;
        }

        for msg in msgs {
            let target = SocketAddrV4::new(msg.destination, msg.unicast_port);
            let (msg_type_id, _) = msg.data.message_type_id();
            let mut state = dispatcher.state.write().await;
            let seq = state.current_seq;
            let header = management_header(&dispatcher.node_config, msg_type_id, seq);
            let packet = Packet { header, data: msg.data };
            trace!("Sending packet to {}: {:?}", target, packet);
            match packet.to_bytes() {
                Ok(bytes) => {
                    socket.send_to(&bytes, target).await.expect("Could not send packet!");
                }
                Err(err) => error!("Serializing malformed packet {:?} \n caused {}", packet, err),
            }
            (state.current_seq, _) = state.current_seq.overflowing_add(1);
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn broadcast(dispatcher: Arc<Dispatcher>, broadcast_socket: Arc<UdpSocket>) {
    let ipv4_addrs = best_local_ipv4_addrs()
        .expect("Could not get local IPv4 addresses")
        .iter().map(|net| SocketAddr::V4(SocketAddrV4::new(net.broadcast(), 60_000)))
        .collect::<HashSet<_>>();
    trace!("Broadcasting opt in packets to {:?}", ipv4_addrs);
    let mut tick = interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        trace!("Sending opt in packets...");

        let mut dispatcher_state = dispatcher.state.write().await;
        let config = dispatcher.node_config;
        let seq = dispatcher_state.current_seq;
        let payload = opt_in_packet(&config, &dispatcher_state, seq)
            .expect("TCNet: Could not serialize opt in packet");
        for addr in &ipv4_addrs {
            let _ = broadcast_socket.send_to(&payload, addr).await;
            (dispatcher_state.current_seq, _) = dispatcher_state.current_seq.overflowing_add(1);
        }

        trace!("Sent opt in packet");
    }
}

async fn timeout_foreign_nodes(node: Arc<Dispatcher>) {
    let mut tick = interval(Duration::from_secs(1));
    loop {
        let secs = timestamp_secs();
        let mut state = node.state.write().await;
        state.discovered_nodes.retain(|_, foreign_node| {
            let keep = foreign_node.last_seen + 10 > secs;
            if !keep {
                warn!("Node {} timed out", foreign_node.address);
            }
            keep
        });
        publish_nodes_snapshot(&state, &node.nodes_buf_input);
        tick.tick().await;
    }
}

pub fn timestamp_micros() -> u32 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("time should go forward");
    since_the_epoch.subsec_micros()
}

pub fn timestamp_secs() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("time should go forward");
    since_the_epoch.as_secs()
}