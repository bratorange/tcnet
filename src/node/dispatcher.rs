use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use log::{error, info, trace, warn};
use tokio::time::interval;
use tokio::spawn;
use tokio::sync::RwLock;
use getifs::best_local_ipv4_addrs;
use kanal::Receiver;
use crate::application::{ApplicationMessage, ApplicationNode};
use crate::node::{ApplicationConfig, DynamicNodeState, ForeignNode};
use crate::node::tcnet_packet::{opt_in_node_config, opt_in_packet, Packet};
use crate::node::tcnet_packet_serde::Data::{OptIn, OptOut};
use crate::node::tcnet_packet_serde::NodeId;

pub struct OutgoingMessage {
    pub destination: SocketAddr,
    pub packet: Packet,
}

#[derive(Clone)]
pub struct Dispatcher {
    pub(crate) application_nodes: Arc<RwLock<HashMap<NodeId, ApplicationNode>>>,
    pub(crate) bind_address: Ipv4Addr,
    pub unicast_port: u16,
    pub(crate) state: Arc<RwLock<DynamicNodeState>>,
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

                        let incoming_node_id = packet.header.node_id;
                        // send data to its respective application
                        let send_result = if let Some(application) =
                            &dispatcher.application_nodes.write().await.get_mut(&packet.header.node_id) {
                            application.incoming_tx.send(packet)
                        } else {
                            warn!("Got message for unknown application: {:?}", &packet.header.node_id);
                            Ok(())
                        };
                        if send_result.is_err(){
                            warn!("Application {} does not listen for messages anymore, removing it from the network", &incoming_node_id);
                            remove_application(&dispatcher, &incoming_node_id).await;
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

pub async fn add_application(
    dispatcher: Arc<Dispatcher>,
    application_config: ApplicationConfig,
) -> (Receiver<ApplicationMessage>, kanal::Sender<ApplicationMessage>) {
    let (incoming_tx, incoming_rx) = kanal::bounded(100);
    let (outgoing_tx, outgoing_rx) = kanal::bounded(100);
    let application_node = ApplicationNode { dispatcher: dispatcher.clone(), config: application_config, incoming_tx, outgoing_rx };
    dispatcher.application_nodes.write().await.insert(application_node.config.node_id, application_node);
    (incoming_rx, outgoing_tx)
}

pub async fn remove_application(dispatcher: &Arc<Dispatcher>, node_id: &NodeId) {
    let mut applications_lock = dispatcher.application_nodes.write().await;
    if let Some(app_node) =applications_lock.get_mut(node_id){
        // TODO send opt out message here
        applications_lock.remove(node_id);
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