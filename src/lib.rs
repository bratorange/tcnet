//! # tcnet
//!
//! A Rust implementation of the **TCNet** UDP protocol used by professional
//! DJ / VJ gear (Pioneer ProDJ-adjacent) for synchronising playback state,
//! mixer state, beat grids, waveforms and timecode between networked nodes.
//!
//! This crate speaks protocol version **3.6** (the value carried in every
//! outgoing [`ManagementHeader`](crate::protocol::ManagementHeader)).  Per-field
//! introduction versions ("FLAMEs" in spec language) are tagged via the
//! [`spec_version`] module so consumers can reason about cross-version
//! compatibility at compile time.
//!
//! The full TCNet V3.5.1B spec is vendored under `docs/spec/`.
//!
//! ## Quick start
//!
//! ```no_run
//! use std::thread::sleep;
//! use std::time::Duration;
//! use tcnet::api::{NodeBuilder, Slave};
//! use tcnet::V3_6;
//!
//! let mut node = NodeBuilder::<Slave, V3_6>::new()
//!     .with_local_ip([127, 0, 0, 1].into())
//!     .spawn()
//!     .expect("node spawn");
//!
//! loop {
//!     let snap = node.snapshot();
//!     for peer in &snap.peers {
//!         if peer.has_dj_controller {
//!             if let Some(layers) = node.layers_for(peer.address) {
//!                 for (i, layer) in layers.iter().enumerate() {
//!                     println!("L{}: {:?} @ {:.1} BPM",
//!                              i + 1, layer.state, layer.bpm.as_f32());
//!                 }
//!             }
//!         }
//!     }
//!     sleep(Duration::from_secs(1));
//! }
//! ```
//!
//! For an active-broadcaster role (emulate a virtual CDJ), parameterise the
//! builder on [`api::Master`] instead — the returned
//! [`Node<Master, V3_6>`](crate::api::Node) `Deref`s to the inner broadcaster
//! handle whose `set_*` / `load_track` / `broadcast_*` methods drive the wire.
//!
//! ## Architecture
//!
//! [`api::Node<R, V>`] is a typed handle (role gating Slave/Master/Auto,
//! FLAME gating by [`spec_version`]) over the dispatcher runtime. The
//! dispatcher owns the four UDP sockets (60000 / 60001 / 60002 / 65023+),
//! drives discovery, Time / Status / Metrics emission, the TimeSync
//! handshake ([`proto::time_sync`]) and master election
//! ([`session::election`]). Peer state is lock-free: an
//! `ArcSwap<HashMap<_, Arc<ForeignNode>>>` published by the dispatcher and
//! read by many — no `Mutex` / `RwLock` anywhere in the crate.
//!
//! ## Modules
//!
//! * [`api`] — typed `Node<R: Role, V: SpecVersion>` + `NodeBuilder` (start here).
//! * [`active_node`] — [`ActiveDJNode`](active_node::ActiveDJNode), the Master
//!   broadcaster handle (`set_*` / `load_track` / `broadcast_*`) that
//!   `Node<Master>` derefs to.
//! * [`view`] — per-peer read view of a foreign DJ controller backing
//!   [`Node::layers_for`](crate::api::Node::layers_for); the `'static`
//!   [`WaveformRequester`](view::WaveformRequester) handle lives here.
//! * [`spec_version`] — `SpecVersion` markers, `Flame` per-field introduction
//!   tags, `IncludesFlame<F>` relation for compile-time version gating,
//!   `PeerVersion` runtime carrier.
//! * [`protocol`] — wire-format payload types: every packet struct, plus
//!   helper newtypes ([`LayerId`], [`LayerState`], [`Bpm`], [`Speed`],
//!   [`AsciiString`], …) and the [`ManagementHeader`](crate::protocol::ManagementHeader).
//! * [`session`] — master-election state machine ([`session::election`]).
//! * [`proto`] — the [`TimeSync`](proto::time_sync) handshake with
//!   spec-page-8 clock-offset computation.
//!
//! ## Behaviour summary
//!
//! Every spec-defined runtime behaviour is implemented and runs:
//!
//! * **Discovery** — OptIn broadcast on port 60000 every 1 s + per-peer
//!   unicast every 1 s.  OptOut broadcast on shutdown.  Peers timeout after
//!   10 s of silence.
//! * **Time** — Master broadcasts on 60001 every 20 ms (spec range 1–40 ms)
//!   + unicast to each discovered node.
//! * **Status** — Master broadcasts on 60000 every 1 s + unicast to slaves.
//! * **Metrics / Meta / Mixer** — unicast to each slave per spec cadences.
//! * **TimeSync** — handshake initiator ticks every 1 s and targets the
//!   most-recently-seen peer whose 5 s per-peer cooldown has elapsed (so
//!   short-lived peers get serviced before they vanish); inbound step=0
//!   replies stamped with our current `header.timestamp`; inbound step=1
//!   resolves the clock offset per spec page 8 formula
//!   (`Delay = (Current timer − Remote timestamp) / 2`,
//!   `time_of_remote_now = responder_send_ts + Delay`).  Result readable
//!   via [`Node::clock_offset_for`](crate::api::Node::clock_offset_for).
//! * **Master election** — 1 Hz driver builds the candidate set from peers
//!   announcing `NodeType::{Master, Auto}`; tie-break by uptime descending,
//!   then announce time ascending, then node id ascending.  Result readable
//!   via [`Node::election_state`](crate::api::Node::election_state).
//! * **Request / Response** — Slaves request waveform / beat-grid / cue /
//!   artwork from `Node<Slave, V3_6>::request_*(addr, layer).await`;
//!   Masters serve the matching response from a pre-populated cache, or
//!   reply with `ErrorNotification(014, EMPTY)` per spec.
//!
//! ## Status
//!
//! Production-untested; expect breaking changes between minor versions while
//! the public API stabilises.  Wire-format conformance is validated against
//! the spec and against PRO DJ LINK Bridge in a manual smoke-test loop.

use crate::node::dispatcher::{Dispatcher, start_node};
use crate::node::dj_controller::OutgoingRequest;
use crate::node::response_data::{ResponseDataStore, SharedResponseData};
use crate::node::tcnet_packet::Data;
use crate::node::ForeignNode;
use crate::protocol::NodeId;
use std::net::SocketAddrV4;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;
use tokio::runtime::Runtime;

pub mod active_node;
pub mod api;
mod node;
pub mod proto;
pub mod protocol;
pub mod session;
pub mod spec_version;
#[cfg(test)]
mod tests;
pub mod view;

pub use active_node::{ActiveDJNode, HotCue, TrackMeta};
pub use api::{Node, NodeBuilder, NodeError, NodeSnapshot, PeerInfo};
pub use node::ApplicationConfig;
pub use node::dj_controller::{
    ChannelSnapshot, DjControllerState, LayerSnapshot, MixerSnapshot, TimeoutError,
};
pub use protocol::{
    AsciiString, BeatGridEntry, BeatGridHeader, BigWaveformData, Bpm, CueData, CueEntry, LayerId,
    LayerState, NodeOptions, NodeType, RequestDataType, SmallWaveformData, SmpteMode, Speed,
    WireError,
};
pub use spec_version::{
    Flame, IncludesFlame, PeerVersion, SpecVersion,
    // Versions
    V1_0, V2_0, V2_1, V3_0, V3_1, V3_2, V3_3, V3_3_1, V3_3_2, V3_3_3, V3_4_1, V3_4_2, V3_5,
    V3_5_1, V3_6,
    // Flames
    ArtworkFileFlame, BaseFlame, BcolorExplanationFlame, BeatGridInfoFlame, CueDataFlame,
    CueExtendedFlame, FaderOnAirFlame, LayerNameFlame, MetadataUtf32Flame, MixerDataFlame,
    MixerExtendedFlame, NodeOptionsFlame, OptInVendorFlame, SmallBigWaveformFlame,
    SmpteInTimePacketFlame, UnicastOptInOutFlame,
};
pub use view::WaveformRequester;
pub(crate) use view::DjControllerView;

/// Snapshot of a foreign node discovered on the network through TCNet OptIn
/// broadcasts. Surfaced to callers as [`PeerInfo`] via
/// [`Node::snapshot`](crate::api::Node::snapshot).
#[derive(Clone, Debug)]
pub struct ForeignNodeInfo {
    /// The node's listener address (IP + unicast port reported in its OptIn packet).
    pub address: SocketAddrV4,
    /// Wall-clock timestamp (UNIX seconds) of the last packet received from this node.
    /// Nodes silent for ≥ 10 s are dropped from the active set automatically.
    pub last_seen: u64,
    /// The 16-bit node identifier reported in the node's `ManagementHeader`.
    pub node_id: NodeId,
    /// `true` once any DJ-controller-class packet (Status / Metrics / Mixer / etc.)
    /// has been received from this node — meaning per-layer reads via
    /// [`Node::layers_for`](crate::api::Node::layers_for) are available.
    pub has_dj_controller: bool,
}

impl From<&ForeignNode> for ForeignNodeInfo {
    fn from(n: &ForeignNode) -> Self {
        ForeignNodeInfo {
            address: n.address(),
            last_seen: n.last_seen(),
            node_id: n.config().node_id,
            has_dj_controller: n.has_dj_controller(),
        }
    }
}

/// Internal engine behind the public [`Node`](crate::api::Node) handle.
///
/// Constructed by [`NodeBuilder::spawn`](crate::api::NodeBuilder::spawn);
/// not part of the public surface. Spawns a dedicated single-threaded tokio
/// runtime, binds the four UDP sockets used by TCNet (broadcast on
/// 60000 / 60001 / 60002, plus a configurable unicast port — the dispatcher
/// falls back to the next free port if any are taken) and starts the
/// discovery loop, which emits an OptIn broadcast every second and listens
/// for replies from peers.
///
/// `Node` drives it through two paths:
///
/// * [`get_controller_view`](Self::get_controller_view) attaches a reader to
///   a foreign DJ controller node (backs `Node::layers_for` / `request_*`).
/// * [`create_active_node`](Self::create_active_node) starts broadcasting
///   this process's playback state (backs `Node<Master>`).
pub(crate) struct TCNetClient {
    _runtime: Runtime,
    dispatcher: Arc<Dispatcher>,
    /// Wait-free atomic snapshot of the published discovered-nodes vector.
    nodes_snapshot: Arc<arc_swap::ArcSwap<Vec<ForeignNodeInfo>>>,
    active_broadcast_tx: kanal::Sender<Data>,
    active_slave_unicast_tx: kanal::Sender<Data>,
    active_time_tx: kanal::Sender<Data>,
    response_data: SharedResponseData,
}

impl TCNetClient {
    /// Construct a new client and start the network dispatcher.
    ///
    /// The `node_config` describes how this node will identify itself to peers
    /// (node id, vendor / application name, node options, bind address). See
    /// [`ApplicationConfig`] for the field semantics; `ApplicationConfig::default()`
    /// produces a usable configuration that binds to `0.0.0.0:65023`.
    pub fn new(node_config: ApplicationConfig) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("tcnet")
            .enable_all()
            .build()
            .expect("Could not start tokio runtime");

        let (outgoing_tx, outgoing_rx) = kanal::bounded::<OutgoingRequest>(256);
        let nodes_snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(
            Vec::<ForeignNodeInfo>::new(),
        ));
        let (active_broadcast_tx, active_broadcast_rx) = kanal::bounded::<Data>(512);
        let (active_slave_unicast_tx, active_slave_unicast_rx) = kanal::bounded::<Data>(512);
        let (active_time_tx, active_time_rx) = kanal::bounded::<Data>(512);
        let response_data: SharedResponseData = Arc::new(ResponseDataStore::default());

        let dispatcher = Arc::new(Dispatcher {
            node_config,
            bind_address: node_config.address,
            actual_unicast_port: AtomicU16::new(node_config.address.port()),
            current_seq: std::sync::atomic::AtomicU8::new(0),
            uptime: AtomicU16::new(0),
            state: Arc::new(arc_swap::ArcSwap::from_pointee(
                std::collections::HashMap::new(),
            )),
            outgoing_tx,
            outgoing_rx,
            nodes_snapshot: nodes_snapshot.clone(),
            active_broadcast_rx,
            active_slave_unicast_rx,
            active_time_rx,
            response_data: response_data.clone(),
            broadcast_targets: arc_swap::ArcSwap::from_pointee(Vec::new()),
            pending_time_sync: crate::node::dispatcher::PendingTimeSyncStore::new(),
            clock_offsets: crate::node::dispatcher::ClockOffsetStore::new(),
            election: crate::node::dispatcher::ElectionStore::new(),
        });

        runtime.spawn(start_node(dispatcher.clone()));

        Self {
            _runtime: runtime,
            dispatcher,
            nodes_snapshot,
            active_broadcast_tx,
            active_slave_unicast_tx,
            active_time_tx,
            response_data,
        }
    }

    /// Lock-free read of the published foreign-node list.
    pub fn nodes_snapshot_arc(&self) -> std::sync::Arc<Vec<ForeignNodeInfo>> {
        self.nodes_snapshot.load_full()
    }

    /// The [`ApplicationConfig`] this client was constructed with.
    pub fn node_config(&self) -> ApplicationConfig {
        self.dispatcher.node_config
    }

    /// Most recent successful TimeSync result for `peer`, or `None`
    /// if no handshake has completed.
    ///
    /// The dispatcher periodically initiates TimeSync(step=0) against
    /// every active peer (~5 s round-robin); replies are matched
    /// against in-flight pending slots and the resolved
    /// [`ClockOffset`](crate::proto::ClockOffset) is published here.
    pub fn clock_offset_for(&self, peer: SocketAddrV4) -> Option<crate::proto::ClockOffset> {
        self.dispatcher.clock_offsets.get(&peer)
    }

    /// Current master-election state.  Returns
    /// [`ElectionState::Watching`](crate::session::ElectionState)
    /// if no candidates have been observed yet.  The election driver
    /// re-evaluates once per second from the current peer set.
    pub fn election_state(&self) -> crate::session::ElectionState {
        self.dispatcher.election.load()
    }

    /// Attach a reader to the foreign DJ controller node at `addr`.
    ///
    /// Returns `None` if no node is currently registered at that address, if no
    /// DJ-class packets have been received from it yet, **or** if a view has
    /// already been taken for the same node — each node's triple-buffer output
    /// can only be claimed once, so the second call returns `None`.
    pub fn get_controller_view(&self, addr: SocketAddrV4) -> Option<DjControllerView> {
        let map = self.dispatcher.state.load_full();
        // The peer map is keyed by `(Ipv4Addr, NodeId)`, but callers hold
        // the announced unicast address — resolve it by scanning values.
        let ctrl = map
            .values()
            .find(|n| n.address() == addr)?
            .dj_controller()?;
        Some(DjControllerView::new(
            ctrl.state.clone(),
            ctrl.request_tx.clone(),
        ))
    }

    /// Return a [`DjControllerView`] for the first discovered node that has a
    /// DJ controller attached. Test-only convenience — production code attaches
    /// to a specific peer address via [`get_controller_view`](Self::get_controller_view).
    #[cfg(test)]
    pub fn get_any_controller_view(&self) -> Option<DjControllerView> {
        let map = self.dispatcher.state.load_full();
        for node in map.values() {
            if let Some(ctrl) = node.dj_controller() {
                return Some(DjControllerView::new(
                    ctrl.state.clone(),
                    ctrl.request_tx.clone(),
                ));
            }
        }
        None
    }

    /// Return a handle to the internal tokio runtime.
    ///
    /// Useful for embedders that want to schedule supplementary async work
    /// (e.g. background waveform polling) without bringing up a second
    /// runtime — call `runtime_handle().spawn(...)`.
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self._runtime.handle().clone()
    }

    /// Create an [`ActiveDJNode`] that broadcasts this process's playback and
    /// mixer state over TCNet.
    ///
    /// The active node shares the underlying sockets, runtime and dispatcher
    /// with this client — every TCNetClient can have at most one ActiveDJNode
    /// active at a time.
    pub fn create_active_node(&self) -> ActiveDJNode {
        ActiveDJNode::new(
            self.active_broadcast_tx.clone(),
            self.active_slave_unicast_tx.clone(),
            self.active_time_tx.clone(),
            self.response_data.clone(),
            &self._runtime,
        )
    }
}

impl Drop for TCNetClient {
    /// Best-effort OptOut broadcast on shutdown.
    ///
    /// Spec V3.5.1B page 5: a node should broadcast (and unicast) one OptOut
    /// packet when leaving the network so peers can drop it immediately
    /// instead of waiting for the 10 s silence timeout. We do this here
    /// synchronously using a fresh `std::net::UdpSocket`, because by the
    /// time `Drop` runs the embedded tokio runtime may be tearing down its
    /// own background tasks and we cannot rely on the dispatcher's send
    /// path being alive.
    fn drop(&mut self) {
        use crate::node::tcnet_packet::management_header;
        use crate::protocol::OptOutData;
        use deku::DekuContainerWrite;
        use std::sync::atomic::Ordering;

        // Drop must not touch the tokio runtime: by the time we run here
        // the dispatcher's tasks may already be shutting down, and a
        // `block_on` would deadlock the worker. Everything below is
        // wait-free and uses only atomic state.
        let cfg = self.dispatcher.node_config;
        let unicast_port = self.dispatcher.actual_unicast_port.load(Ordering::Relaxed);
        let bcast_targets = self.dispatcher.broadcast_targets.load_full();

        // node_count / seq are non-critical on OptOut — the receiving peer
        // identifies us via the header's node_id and drops our entry. A
        // best-effort `(1, 0)` matches the typical lone-departing-node
        // shape on the wire.
        let header = management_header(&cfg, 3, 0);
        let data = OptOutData {
            node_count: 1,
            node_listener_port: unicast_port,
        };
        let bytes = match (header.to_bytes(), data.to_bytes()) {
            (Ok(h), Ok(d)) => [h, d].concat(),
            _ => return,
        };

        // Fan out via a one-shot sync socket. Best effort — failures are
        // ignored because the runtime is mid-shutdown.
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            let _ = sock.set_broadcast(true);
            for addr in bcast_targets.iter() {
                let _ = sock.send_to(&bytes, addr);
            }
        }
    }
}
