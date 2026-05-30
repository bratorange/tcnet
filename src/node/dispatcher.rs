use crate::ForeignNodeInfo;
use crate::node::dj_controller::{DjController, OutgoingRequest};
use crate::node::response_data::SharedResponseData;
use crate::node::tcnet_packet::Data;
use crate::node::tcnet_packet::Data::{OptIn, OptOut};
use crate::node::tcnet_packet::{
    Packet, config_from_header, management_header, node_config_from_opt_in, opt_in_packet,
};
use crate::node::{ApplicationConfig, DynamicNodeState, ForeignNode};
use crate::protocol::{LayerId, NodeType, RequestDataType};
use deku::DekuContainerWrite;
use getifs::best_local_ipv4_addrs;
use log::{error, info, trace, warn};
use std::collections::HashSet;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio::sync::RwLock;
use tokio::time::interval;

pub struct Dispatcher {
    pub(crate) node_config: ApplicationConfig,
    pub(crate) bind_address: SocketAddrV4,
    /// Actual unicast port bound at runtime; may differ from bind_address.port() when falling back.
    pub(crate) actual_unicast_port: AtomicU16,
    /// Monotonic SEQ counter for every outgoing packet (single shared
    /// source — see ARCHITECTURE.md §5.2). Wraps at u8 boundary by design.
    pub(crate) current_seq: AtomicU8,
    /// Node uptime in seconds, rolling over at 12 h (43_200) per spec
    /// page 4. Owned by `timeout_foreign_nodes` (1 Hz tick), read by
    /// `opt_in_packet`.
    pub(crate) uptime: AtomicU16,
    /// Discovered foreign peers. Reads & writes still serialise through a
    /// tokio RwLock; lifting this to a single-actor session task is the
    /// next architectural step (ARCHITECTURE.md §5.1).
    pub(crate) state: Arc<RwLock<DynamicNodeState>>,
    pub(crate) outgoing_tx: kanal::Sender<OutgoingRequest>,
    pub(crate) outgoing_rx: kanal::Receiver<OutgoingRequest>,
    /// Published list of discovered foreign nodes. Single writer (the
    /// dispatcher main flow), many readers via `TCNetClient::active_nodes`.
    /// Wait-free atomic pointer swap — no lock.
    pub(crate) nodes_snapshot: Arc<ArcSwap<Vec<ForeignNodeInfo>>>,
    /// Packets broadcast on port 60000 (Status, OptIn).
    pub(crate) active_broadcast_rx: kanal::Receiver<Data>,
    /// Packets unicast to each slave node (Metrics, Meta, Mixer).
    pub(crate) active_slave_unicast_rx: kanal::Receiver<Data>,
    /// Packets broadcast on port 60001 (Time).
    pub(crate) active_time_rx: kanal::Receiver<Data>,
    /// Shared store for pre-built request-response payloads.
    pub(crate) response_data: SharedResponseData,
    /// Broadcast destinations for OptIn / OptOut. Published by `broadcast()`
    /// at start-up so a synchronous `Drop` impl can reuse the same fan-out
    /// (including the loopback fallback) without re-running
    /// `best_local_ipv4_addrs`. Single writer / many readers — wait-free
    /// via `ArcSwap` instead of a `Mutex`.
    pub(crate) broadcast_targets: ArcSwap<Vec<SocketAddrV4>>,
}

/// Try to bind a UDP socket at `preferred_port`, incrementing until a free port is found.
/// Enables broadcast if `enable_broadcast` is set.
async fn bind_with_fallback(
    ip: Ipv4Addr,
    preferred_port: u16,
    enable_broadcast: bool,
) -> (UdpSocket, u16) {
    let mut port = preferred_port;
    loop {
        match UdpSocket::bind(SocketAddrV4::new(ip, port)).await {
            Ok(socket) => {
                if enable_broadcast {
                    socket.set_broadcast(true).expect("set_broadcast failed");
                }
                let actual = socket.local_addr().map(|a| a.port()).unwrap_or(port);
                if actual != preferred_port && preferred_port != 0 {
                    info!("Port {} busy; using {} instead", preferred_port, actual);
                }
                return (socket, actual);
            }
            Err(_) if port < 65_534 => port += 1,
            Err(e) => panic!(
                "Could not bind any port starting from {}: {}",
                preferred_port, e
            ),
        }
    }
}

pub async fn start_node(dispatcher: Arc<Dispatcher>) {
    trace!("binding sockets...");
    let ip = *dispatcher.bind_address.ip();

    let (s, _) = bind_with_fallback(ip, 60_000, true).await;
    let broadcast_socket = Arc::new(s);

    let (s, _) = bind_with_fallback(ip, 60_001, true).await;
    let time_broadcast_socket = Arc::new(s);

    let (s, _) = bind_with_fallback(ip, 60_002, true).await;
    let broadcast_socket2 = Arc::new(s);

    let (s, unicast_port) = bind_with_fallback(ip, dispatcher.bind_address.port(), false).await;
    dispatcher
        .actual_unicast_port
        .store(unicast_port, Ordering::Relaxed);
    let unicast_socket = Arc::new(s);

    trace!("Starting network processing");

    spawn(broadcast(dispatcher.clone(), broadcast_socket.clone()));
    spawn(listen(dispatcher.clone(), broadcast_socket.clone()));
    spawn(listen(dispatcher.clone(), broadcast_socket2.clone()));
    spawn(listen(dispatcher.clone(), time_broadcast_socket.clone()));
    spawn(listen(dispatcher.clone(), unicast_socket.clone()));
    // Use broadcast_socket (port 60000) for outgoing responses so the source port
    // matches the OptIn source, keeping all packets under one discovery key.
    spawn(send(dispatcher.clone(), broadcast_socket.clone()));
    spawn(timeout_foreign_nodes(dispatcher.clone()));
    spawn(active_broadcast(
        dispatcher.clone(),
        broadcast_socket.clone(),
        time_broadcast_socket.clone(),
    ));
}

fn is_dj_packet(data: &Data) -> bool {
    matches!(
        data,
        Data::Status(_)
            | Data::Metrics(_)
            | Data::Meta(_)
            | Data::BeatGrid(_)
            | Data::Cue(_)
            | Data::SmallWaveform(_)
            | Data::BigWaveform(_)
            | Data::Mixer(_)
            | Data::ArtworkFile(_)
            | Data::Time(_)
    )
}

fn publish_nodes_snapshot(
    state: &DynamicNodeState,
    nodes_snapshot: &Arc<ArcSwap<Vec<ForeignNodeInfo>>>,
) {
    let snapshot: Vec<ForeignNodeInfo> = state
        .discovered_nodes
        .values()
        .map(ForeignNodeInfo::from)
        .collect();
    nodes_snapshot.store(Arc::new(snapshot));
}

async fn listen(dispatcher: Arc<Dispatcher>, socket: Arc<UdpSocket>) -> io::Result<()> {
    let mut buffer = [0; 8192];
    info!("Start Listening for packets on {}", socket.local_addr()?);
    loop {
        match socket.recv_from(&mut buffer).await {
            Ok((size, src)) => {
                trace!("Received {} bytes from {}", size, src);
                match Packet::deserialize_packet(&buffer[..size]) {
                    Ok(packet) => {
                        trace!("Received packet: {:?}", packet);

                        let src_addr = match src {
                            SocketAddr::V4(addr) => addr,
                            _ => unreachable!(),
                        };
                        let src_ip = *src_addr.ip();

                        match &packet.data {
                            OptIn(opt_in_data) => {
                                let node_config =
                                    node_config_from_opt_in(src_ip, &packet.header, opt_in_data);
                                let mut state = dispatcher.state.write().await;
                                let node =
                                    state
                                        .discovered_nodes
                                        .entry(src_addr)
                                        .or_insert(ForeignNode {
                                            last_seen: 0,
                                            address: node_config.address,
                                            config: node_config,
                                            dj_controller: None,
                                        });
                                node.last_seen = timestamp_secs();
                                // Update address/config on each OptIn in case port changed.
                                node.address = node_config.address;
                                node.config = node_config;
                                publish_nodes_snapshot(&state, &dispatcher.nodes_snapshot);
                            }

                            OptOut(_) => {
                                let mut state = dispatcher.state.write().await;
                                let removed = state.discovered_nodes.remove(&src_addr);
                                if removed.is_none() {
                                    warn!(
                                        "Node {} has opted out despite not being part of the network (anymore)",
                                        src_ip
                                    );
                                } else {
                                    info!("Node {} has opted out of the network", src_ip);
                                }
                                publish_nodes_snapshot(&state, &dispatcher.nodes_snapshot);
                            }

                            Data::Request(req) => {
                                let (dest, packets) = {
                                    let state = dispatcher.state.read().await;
                                    let dest = state
                                        .discovered_nodes
                                        .get(&src_addr)
                                        .or_else(|| {
                                            state
                                                .discovered_nodes
                                                .iter()
                                                .find(|(k, _)| *k.ip() == src_ip)
                                                .map(|(_, v)| v)
                                        })
                                        .map(|n| n.address)
                                        .unwrap_or(SocketAddrV4::new(src_ip, 65_023));
                                    let packets = build_request_response(
                                        req.data_type,
                                        req.layer,
                                        &dispatcher.response_data,
                                    );
                                    (dest, packets)
                                };
                                if packets.is_empty() {
                                    // Spec V3.5.1B page 9: when there is no
                                    // payload to return, reply with an
                                    // ErrorNotification (code 014 = EMPTY,
                                    // message_type 20 = original Request)
                                    // so the peer can fail fast instead of
                                    // hitting its 5 s read timeout.
                                    let err = Data::ErrorNotification(
                                        crate::protocol::ErrorNotificationData::new(
                                            req.data_type,
                                            req.layer.as_packet_id(),
                                            14,
                                            20,
                                        ),
                                    );
                                    let _ = dispatcher.outgoing_tx.try_send(OutgoingRequest {
                                        destination: dest,
                                        data: err,
                                    });
                                } else {
                                    for pkt in packets {
                                        let _ = dispatcher.outgoing_tx.try_send(OutgoingRequest {
                                            destination: dest,
                                            data: pkt,
                                        });
                                    }
                                }
                            }

                            _ => {}
                        }

                        if is_dj_packet(&packet.data) {
                            let mut state = dispatcher.state.write().await;
                            let mut created_new_ctrl = false;
                            // Two-pass routing: prefer exact src_addr match, then any
                            // entry sharing src_ip. This routes waveform responses
                            // (source port 65023) and Time broadcasts (source port
                            // 60001) to the single DjController registered via OptIn
                            // (source port 60000), preventing spurious duplicate entries.
                            let key = if state.discovered_nodes.contains_key(&src_addr) {
                                src_addr
                            } else if let Some(&k) =
                                state.discovered_nodes.keys().find(|k| *k.ip() == src_ip)
                            {
                                k
                            } else {
                                src_addr
                            };
                            let node_addr = state
                                .discovered_nodes
                                .get(&key)
                                .map(|n| n.address)
                                .unwrap_or(SocketAddrV4::new(src_ip, 65_023));
                            // When a DJ packet arrives before its OptIn we still
                            // need to publish a useful node_id/name etc — lift
                            // them from the header instead of ApplicationConfig::default.
                            let header_config = config_from_header(&packet.header, src_ip);
                            state
                                .discovered_nodes
                                .entry(key)
                                .and_modify(|foreign_node| {
                                    if foreign_node.dj_controller.is_none() {
                                        let (ctrl, task_fut) = DjController::new(
                                            dispatcher.outgoing_tx.clone(),
                                            foreign_node.address,
                                        );
                                        foreign_node.dj_controller = Some(ctrl);
                                        spawn(task_fut);
                                        created_new_ctrl = true;
                                    }
                                    if let Some(ctrl) = &foreign_node.dj_controller {
                                        let _ = ctrl.packet_tx.try_send(packet);
                                    }
                                    foreign_node.last_seen = timestamp_secs();
                                })
                                .or_insert_with(|| {
                                    let (ctrl, task_fut) = DjController::new(
                                        dispatcher.outgoing_tx.clone(),
                                        node_addr,
                                    );
                                    created_new_ctrl = true;
                                    spawn(task_fut);
                                    ForeignNode {
                                        last_seen: timestamp_secs(),
                                        address: node_addr,
                                        config: header_config,
                                        dj_controller: Some(ctrl),
                                    }
                                });

                            if created_new_ctrl {
                                publish_nodes_snapshot(&state, &dispatcher.nodes_snapshot);
                            }
                        }
                    }
                    Err(e) => {
                        trace!("Incoming packet deserialization failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                error!("Network error: {}", e);
            }
        }
    }
}

/// Builds the response packets for a REQUEST packet by snapshotting the
/// relevant `ArcSwap` fields. Wait-free for the dispatcher.
fn build_request_response(
    data_type: RequestDataType,
    layer: LayerId,
    rd: &crate::node::response_data::ResponseDataStore,
) -> Vec<Data> {
    let idx = layer.index();
    let ld = &rd.layers[idx];

    // Helper: snapshot an `ArcSwap<Option<Data>>` into a one-element Vec or empty.
    fn opt_to_vec(arc: arc_swap::Guard<Arc<Option<Data>>>) -> Vec<Data> {
        match (**arc).as_ref() {
            Some(d) => vec![d.clone()],
            None => Vec::new(),
        }
    }

    match data_type {
        RequestDataType::MetricsData => opt_to_vec(ld.last_metrics.load()),
        RequestDataType::MetaData => opt_to_vec(ld.last_meta.load()),
        RequestDataType::BeatGridData => (**ld.beat_grid_packets.load()).clone(),
        RequestDataType::CueData => opt_to_vec(ld.cue_packet.load()),
        RequestDataType::SmallWaveformData => opt_to_vec(ld.small_waveform_packet.load()),
        RequestDataType::LargeWaveformData => (**ld.big_waveform_packets.load()).clone(),
        RequestDataType::LowResArtworkFile => (**ld.artwork_packets.load()).clone(),
        RequestDataType::MixerData => opt_to_vec(rd.last_mixer.load()),
    }
}

async fn send(dispatcher: Arc<Dispatcher>, socket: Arc<UdpSocket>) {
    loop {
        let mut msgs: Vec<OutgoingRequest> = Vec::new();
        if dispatcher.outgoing_rx.drain_into(&mut msgs).is_err() {
            break;
        }

        for msg in msgs {
            let target = msg.destination;
            let (msg_type_id, _) = msg.data.message_type_id();
            let seq = next_seq(&dispatcher);
            let header = management_header(&dispatcher.node_config, msg_type_id, seq);
            let packet = Packet {
                header,
                data: msg.data,
            };
            trace!("Sending packet to {}: {:?}", target, packet);
            let serde_result = packet.to_bytes();
            match serde_result {
                Ok(bytes) => {
                    if let Err(e) = socket.send_to(&bytes, target).await {
                        // UDP send_to can fail asynchronously (ENETUNREACH,
                        // EMSGSIZE, ECONNREFUSED on connected sockets).
                        // Log and continue — one bad destination must not
                        // bring down the entire send task.
                        warn!("send_to {} failed: {}", target, e);
                    }
                }
                Err(err) => error!("Serializing malformed packet, caused {}", err),
            }
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn broadcast(dispatcher: Arc<Dispatcher>, broadcast_socket: Arc<UdpSocket>) {
    let mut ipv4_addrs = best_local_ipv4_addrs()
        .expect("Could not get local IPv4 addresses")
        .iter()
        .map(|net| SocketAddrV4::new(net.broadcast(), 60_000))
        .collect::<HashSet<_>>();
    // When we fell back from port 60000 (another process owns it), also broadcast on the
    // loopback network so applications like DJ Link Bridge that are bound to 127.0.0.1
    // can discover us.
    let on_fallback_port = broadcast_socket
        .local_addr()
        .map(|a| a.port() != 60_000)
        .unwrap_or(false);
    if on_fallback_port {
        ipv4_addrs.insert(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 60_000));
    }
    // Publish the broadcast destinations on the dispatcher so that the
    // sync `Drop` impl in lib.rs can fan an OptOut packet to the same set.
    // Wait-free atomic swap — no lock.
    dispatcher
        .broadcast_targets
        .store(Arc::new(ipv4_addrs.iter().copied().collect()));
    info!("Broadcasting opt in packets to {:?}", ipv4_addrs);
    let mut tick = interval(Duration::from_secs(1));
    loop {
        tick.tick().await;

        let packets = {
            let dispatcher_state = dispatcher.state.read().await;
            let mut config = dispatcher.node_config;
            config
                .address
                .set_port(dispatcher.actual_unicast_port.load(Ordering::Relaxed));

            // Unicast to each known node's listener port.
            let unicast_targets: Vec<SocketAddrV4> = dispatcher_state
                .discovered_nodes
                .values()
                .map(|n| n.address)
                .collect();
            let mut packets: Vec<(SocketAddrV4, Vec<u8>)> = unicast_targets
                .iter()
                .map(|addr| {
                    let seq = next_seq(&dispatcher);
                    let pkt = opt_in_packet(&config, &dispatcher, seq)
                        .expect("TCNet: Could not serialize opt in packet");
                    (*addr, pkt)
                })
                .collect();

            // Also broadcast.
            for bcast_addr in &ipv4_addrs {
                let seq = next_seq(&dispatcher);
                let pkt = opt_in_packet(&config, &dispatcher, seq)
                    .expect("TCNet: Could not serialize opt in packet");
                packets.push((*bcast_addr, pkt));
            }

            packets
        };
        trace!("Sending opt in packets {:?}", packets);

        for (addr, packet) in &packets {
            let _ = broadcast_socket.send_to(packet, addr).await;
        }

        trace!("Sent opt in packet");
    }
}

async fn timeout_foreign_nodes(node: Arc<Dispatcher>) {
    let mut tick = interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        let secs = timestamp_secs();
        {
            let mut state = node.state.write().await;
            // Spec V3.5.1B page 4: OptIn `uptime` is seconds-since-start and
            // must roll over every 12 h (= 43_200 s). One increment per
            // second matches the 1 Hz tick we are already on.
            let new_uptime = (node.uptime.load(Ordering::Relaxed) + 1) % 43_200;
            node.uptime.store(new_uptime, Ordering::Relaxed);
            state.discovered_nodes.retain(|_, foreign_node| {
                let keep = foreign_node.last_seen + 10 > secs;
                if !keep {
                    warn!("Node {} timed out", foreign_node.address);
                }
                keep
            });
            publish_nodes_snapshot(&state, &node.nodes_snapshot);
        }
    }
}

/// Drains packets queued by `ActiveDJNode` and sends them on the appropriate sockets.
async fn active_broadcast(
    dispatcher: Arc<Dispatcher>,
    socket_60000: Arc<UdpSocket>,
    socket_60001: Arc<UdpSocket>,
) {
    let mut broadcast_addrs_60000: HashSet<SocketAddr> = best_local_ipv4_addrs()
        .expect("Could not get local IPv4 addresses")
        .iter()
        .map(|net| SocketAddr::V4(SocketAddrV4::new(net.broadcast(), 60_000)))
        .collect();
    let on_fallback = socket_60000
        .local_addr()
        .map(|a| a.port() != 60_000)
        .unwrap_or(false);
    if on_fallback {
        broadcast_addrs_60000.insert(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            60_000,
        )));
    }
    let broadcast_addrs_60001: HashSet<SocketAddr> = best_local_ipv4_addrs()
        .expect("Could not get local IPv4 addresses")
        .iter()
        .map(|net| SocketAddr::V4(SocketAddrV4::new(net.broadcast(), 60_001)))
        .collect();

    loop {
        // Read node count and addresses once per loop iteration.
        let (node_count, all_addrs, slave_addrs) = {
            let state = dispatcher.state.read().await;
            let node_count = (state.discovered_nodes.len() + 1) as u16;
            let all_addrs: Vec<SocketAddrV4> =
                state.discovered_nodes.values().map(|n| n.address).collect();
            let slave_addrs: Vec<SocketAddrV4> = state
                .discovered_nodes
                .values()
                .filter(|n| matches!(n.config.node_type, NodeType::Slave | NodeType::Repeater))
                .map(|n| n.address)
                .collect();
            (node_count, all_addrs, slave_addrs)
        };

        // Status packets — broadcast on port 60000, plus unicast to each
        // slave / repeater (spec page 6: "Broadcast on port 60000, Unicast
        // to all slaves"). Master / Auto peers receive the broadcast copy
        // only.
        let mut msgs: Vec<Data> = Vec::new();
        let _ = dispatcher.active_broadcast_rx.drain_into(&mut msgs);
        for mut data in msgs {
            if let Data::Status(ref mut s) = data {
                s.node_count = node_count;
            }
            let (msg_type_id, _) = data.message_type_id();
            let seq = next_seq(&dispatcher);
            let header = management_header(&dispatcher.node_config, msg_type_id, seq);
            let packet = Packet { header, data };
            if let Ok(bytes) = packet.to_bytes() {
                for addr in &broadcast_addrs_60000 {
                    let _ = socket_60000.send_to(&bytes, addr).await;
                }
                for addr in &slave_addrs {
                    let _ = socket_60000.send_to(&bytes, addr).await;
                }
            }
        }

        // Metrics / Meta / Mixer packets — unicast to each slave/repeater node.
        let mut slave_msgs: Vec<Data> = Vec::new();
        let _ = dispatcher
            .active_slave_unicast_rx
            .drain_into(&mut slave_msgs);
        if !slave_msgs.is_empty() {
            for data in slave_msgs {
                let (msg_type_id, _) = data.message_type_id();
                let seq = next_seq(&dispatcher);
                let header = management_header(&dispatcher.node_config, msg_type_id, seq);
                let packet = Packet { header, data };
                if let Ok(bytes) = packet.to_bytes() {
                    for addr in &slave_addrs {
                        let _ = socket_60000.send_to(&bytes, addr).await;
                    }
                }
            }
        }

        // Time packets — broadcast on port 60001 AND unicast to all discovered nodes.
        // Unicast uses socket_60000 so src_addr matches the key used during OptIn discovery.
        // Per spec, time packets shall be unicasted to each known node in addition to broadcast.
        let mut time_msgs: Vec<Data> = Vec::new();
        let _ = dispatcher.active_time_rx.drain_into(&mut time_msgs);
        for data in time_msgs {
            let (msg_type_id, _) = data.message_type_id();
            let seq = next_seq(&dispatcher);
            let header = management_header(&dispatcher.node_config, msg_type_id, seq);
            let packet = Packet { header, data };
            if let Ok(bytes) = packet.to_bytes() {
                for addr in &broadcast_addrs_60001 {
                    trace!("Sending time packet to {}: {:?}", addr, packet);
                    let _ = socket_60001.send_to(&bytes, addr).await;
                }
                for addr in &all_addrs {
                    let _ = socket_60000.send_to(&bytes, addr).await;
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Pull the next sequence number from the shared atomic counter and bump
/// it (wrapping at the u8 boundary). All outgoing packets — broadcasts,
/// unicasts, request responses — draw from this single source so that
/// the SEQ byte on the wire forms one monotonically-increasing stream.
fn next_seq(dispatcher: &Dispatcher) -> u8 {
    dispatcher.current_seq.fetch_add(1, Ordering::Relaxed)
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
