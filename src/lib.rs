//! # tcnet
//!
//! A Rust implementation of the **TCNet** UDP protocol — a network protocol used by
//! professional DJ / VJ gear (Pioneer / ProDJ Link adjacent) for synchronising
//! playback state, mixer state, beat-grid information and waveform previews between
//! networked nodes.
//!
//! This crate covers protocol version `3.6` (the value emitted in every outgoing
//! [`ManagementHeader`](crate::protocol::ManagementHeader)) and supports both roles:
//!
//! * **Passive observer** — discover foreign DJ controller nodes broadcasting on the
//!   network and read their state through [`DjControllerView`]. Useful for VJ tools,
//!   visualisers, lighting controllers, analytics, etc.
//! * **Active broadcaster** — present this process as a virtual DJ node via
//!   [`ActiveDJNode`]: announce up to eight layers of playback, push Time / Status /
//!   Metrics / Meta / Mixer packets, and serve pre-built waveform / beat-grid / cue /
//!   artwork responses on request.
//!
//! ## Specification
//!
//! The packet layouts, message-type IDs and field meanings all follow the official
//! spec, available here:
//!
//! <https://www.tc-supply.com/_files/ugd/b1c714_0b351a4099c14e738f0cd7fcea623265.pdf>
//!
//! Citations elsewhere in these docs link back to that PDF.
//!
//! ## Quick start
//!
//! ```no_run
//! use std::thread::sleep;
//! use std::time::Duration;
//! use tcnet::{ApplicationConfig, TCNetClient};
//!
//! let config = ApplicationConfig::default();
//! let mut client = TCNetClient::new(config);
//!
//! // Wait for a foreign DJ controller to be discovered, then read its state.
//! loop {
//!     // Materialise the snapshot into an owned Vec so that the &mut borrow
//!     // active_nodes() returns is released before we re-borrow `client` to
//!     // call get_controller_view().
//!     let nodes: Vec<_> = client.active_nodes().to_vec();
//!     for node in &nodes {
//!         if node.has_dj_controller {
//!             if let Some(mut view) = client.get_controller_view(node.address) {
//!                 for (i, layer) in view.get_layers().iter().enumerate() {
//!                     println!("L{}: {:?} @ {:.1} BPM", i + 1, layer.state, layer.bpm.as_f32());
//!                 }
//!             }
//!         }
//!     }
//!     sleep(Duration::from_secs(1));
//! }
//! ```
//!
//! ## Architecture
//!
//! ```text
//!                                ┌──────────────────────────────┐
//!                                │ TCNetClient                  │
//!                                │   • spawns tokio runtime     │
//!                                │   • binds UDP sockets        │
//!                                │   • runs OptIn discovery     │
//!  network                       │   • dispatches packets       │
//! ──────────                     │                              │
//!  60000 ── broadcast ◀──────────┤                              │
//!  60001 ── time     ◀──────────►│        Dispatcher            │◀── ActiveDJNode (broadcast role)
//!  60002 ── broadcast ◀──────────┤                              │
//!  port  ── unicast   ◀──────────┤                              │
//!                                │                              │
//!                                │   per-foreign-node triple    │
//!                                │   buffer ──────────────────► │── DjControllerView (read role)
//!                                └──────────────────────────────┘
//! ```
//!
//! ## Module layout
//!
//! ### Legacy single-binary API (current, stable)
//!
//! * [`protocol`] — wire-format types: every packet payload struct, plus helper
//!   types ([`LayerId`], [`LayerState`], [`Bpm`], [`Speed`], …).
//! * [`view`] — read-only consumer view of a discovered foreign node.
//!
//! Most users only need the types re-exported at the crate root.
//!
//! ### Layered next-generation modules (additive, 0.1.x)
//!
//! `ARCHITECTURE.md` describes a six-layer rewrite — wire / transport /
//! session / protocol-machines / domain / runtime — capped with a typed
//! `api::Node<R, V>` surface.  These modules ship today alongside the
//! legacy surface; downstream code can adopt them at its own pace.
//!
//! * [`spec_version`] — `SpecVersion` + `Flame` + `IncludesFlame<F>`
//!   relation; `PeerVersion` runtime carrier.
//! * [`transport`] — `Transport` trait + `Channel` taxonomy +
//!   `MemoryTransport` (loopback) + `UdpTransport` (real network) +
//!   `BufferPool`.
//! * [`session`] — single-actor `SessionTask` with `Peer<…>` state
//!   machine, election FSM, snapshot publication via `ArcSwap`.
//! * [`proto`] — protocol machines: `ChunkedFrame<T>`, `TimeSync`,
//!   `ControlPath`, `TextMessage`, `KeyPress`, `AppSpecificReassembler`,
//!   `Pending<T>` / `RequestError`.
//! * [`domain`] — `DomainLayerSnapshot` + `TimestampOrdered<T>` writer.
//! * [`runtime`] — drift-corrected `Ticker` for the RT hot path.
//! * [`api`] — typed `Node<R: Role, V: SpecVersion>` + `NodeBuilder`.
//!
//! ## Implementation status
//!
//! This crate covers the parts of TCNet v3.6 needed to observe and impersonate
//! a Pioneer-style DJ controller on the network. It is **not** a full
//! reference implementation of every message type defined in the spec.
//!
//! **Implemented:**
//!
//! * Discovery — `OptIn` broadcasts every second on UDP 60000, listening for
//!   peer `OptIn` / `OptOut`, 10-second timeout for stale nodes.
//! * Foreign-node observation — `Status`, `Metrics`, `Meta`, `Mixer` and
//!   `Time` packets are decoded and merged into [`LayerSnapshot`] /
//!   [`MixerSnapshot`], published through a triple buffer to
//!   [`DjControllerView`].
//! * On-demand requests from a [`DjControllerView`] — `SmallWaveform`,
//!   `BigWaveform`, `BeatGrid` (with multi-packet reassembly) and
//!   `LowResArtworkFile`, each with a 5-second timeout.
//! * Active broadcasting via [`ActiveDJNode`] — periodic `Time` (20 ms),
//!   `Status` (1 s) and per-layer `Metrics` (50 ms while playing) emission,
//!   plus on-demand replies to peer `RequestData` packets from a pre-built
//!   response cache (`SmallWaveform`, `BigWaveform`, `BeatGrid`, `Cue`,
//!   `Artwork`, `Mixer`, `Metrics`, `Meta`).
//!
//! **Partial / not implemented:**
//!
//! * **No `OptOut` is emitted** when an `ActiveDJNode` is dropped — peers
//!   currently rely on the 10-second silence timeout to drop us. Incoming
//!   `OptOut` from peers *is* handled.
//! * **`TimeSync`** (message type 10) — struct defined for parsing, but the
//!   handshake is not performed; this crate relies on local system time and
//!   the per-packet microsecond timestamp.
//! * **`ErrorNotification`** (message type 13) — struct defined, never sent
//!   or surfaced to the user when received.
//! * **`Control` / `Text` / `Keyboard` / `AppSpecific`** (message types 101 /
//!   128 / 132 / 30 / 213) — structs defined but neither emitted nor surfaced;
//!   incoming packets are deserialised and dropped.
//! * **Authentication** ([`NodeOptions::NEED_AUTHENTICATION`]) — no
//!   handshake implemented; this crate always operates as an unauthenticated
//!   peer.
//! * **`LayerStatus` and `AutoMasterMode`** are placeholder enums with a
//!   single variant — the spec leaves these mostly unspecified at v3.6.
//! * **Master-election arbitration** — `NodeType` is reported faithfully but
//!   the `Auto`/`Master`/`Slave` election logic is not driven by this crate.
//! * **`MixerData` round-trip** — most fields are surfaced through
//!   [`MixerSnapshot`], but a handful of less-common send-FX / send-return
//!   bytes are read from the wire without a dedicated snapshot field.
//!
//! PRs welcome.

use crate::node::dispatcher::{Dispatcher, start_node};
use crate::node::dj_controller::OutgoingRequest;
use crate::node::response_data::{ResponseDataStore, SharedResponseData};
use crate::node::tcnet_packet::Data;
use crate::node::{DynamicNodeState, ForeignNode};
use crate::protocol::NodeId;
use std::net::SocketAddrV4;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

pub mod active_node;
pub mod api;
pub mod domain;
mod node;
pub mod proto;
pub mod protocol;
pub mod runtime;
pub mod session;
pub mod spec_version;
#[cfg(test)]
mod tests;
pub mod transport;
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
    CueExtendedFlame, FaderOnAirFlame, LayerNameFlame, MetadataUtf16Flame, MixerDataFlame,
    MixerExtendedFlame, NodeOptionsFlame, OptInVendorFlame, SmallBigWaveformFlame,
    SmpteInTimePacketFlame, UnicastOptInOutFlame,
};
pub use view::{DjControllerView, WaveformRequester};

/// Snapshot of a foreign node discovered on the network through TCNet OptIn
/// broadcasts. Returned by [`TCNetClient::active_nodes`].
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
    /// has been received from this node — meaning a [`DjControllerView`] is available.
    pub has_dj_controller: bool,
}

impl From<&ForeignNode> for ForeignNodeInfo {
    fn from(n: &ForeignNode) -> Self {
        ForeignNodeInfo {
            address: n.address,
            last_seen: n.last_seen,
            node_id: n.config.node_id,
            has_dj_controller: n.dj_controller.is_some(),
        }
    }
}

/// Entry point to the TCNet network.
///
/// Constructing a `TCNetClient` spawns a dedicated single-threaded tokio runtime,
/// binds the four UDP sockets used by TCNet (broadcast on 60000 / 60001 / 60002,
/// plus a configurable unicast port — the dispatcher will fall back to the next
/// free port if any of these are taken) and starts the discovery loop, which
/// emits an OptIn broadcast every second and listens for replies from peers.
///
/// From here the two roles split:
///
/// * Call [`get_controller_view`](Self::get_controller_view) (or
///   [`get_any_controller_view`](Self::get_any_controller_view)) to attach a
///   reader to a foreign DJ controller node.
/// * Call [`create_active_node`](Self::create_active_node) to start broadcasting
///   this process's playback state.
///
/// Both can be used at the same time.
pub struct TCNetClient {
    _runtime: Runtime,
    dispatcher: Arc<Dispatcher>,
    /// Wait-free atomic snapshot of the published discovered-nodes vector.
    nodes_snapshot: Arc<arc_swap::ArcSwap<Vec<ForeignNodeInfo>>>,
    /// Cached Arc handle returned by the last `active_nodes()` call so we
    /// can hand out `&[ForeignNodeInfo]` with a stable lifetime.
    cached_nodes: Arc<Vec<ForeignNodeInfo>>,
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
            state: Arc::new(RwLock::new(DynamicNodeState::default())),
            outgoing_tx,
            outgoing_rx,
            nodes_snapshot: nodes_snapshot.clone(),
            active_broadcast_rx,
            active_slave_unicast_rx,
            active_time_rx,
            response_data: response_data.clone(),
            broadcast_targets: arc_swap::ArcSwap::from_pointee(Vec::new()),
        });

        runtime.spawn(start_node(dispatcher.clone()));

        Self {
            _runtime: runtime,
            dispatcher,
            nodes_snapshot,
            cached_nodes: Arc::new(Vec::new()),
            active_broadcast_tx,
            active_slave_unicast_tx,
            active_time_tx,
            response_data,
        }
    }

    /// Return the latest known set of foreign nodes seen on the network.
    ///
    /// This is a snapshot — calling it again later returns an updated list.
    /// Nodes that have not been heard from for ≥ 10 s are dropped automatically.
    pub fn active_nodes(&mut self) -> &[ForeignNodeInfo] {
        // Wait-free load of the most recent published snapshot.
        self.cached_nodes = self.nodes_snapshot.load_full();
        self.cached_nodes.as_slice()
    }

    /// Lock-free read of the foreign-node list — does not refresh
    /// the internal cache and does not require `&mut self`.  Cheaper
    /// when you don't need a stable borrow.
    pub fn nodes_snapshot_arc(&self) -> std::sync::Arc<Vec<ForeignNodeInfo>> {
        self.nodes_snapshot.load_full()
    }

    /// The [`ApplicationConfig`] this client was constructed with.
    pub fn node_config(&self) -> ApplicationConfig {
        self.dispatcher.node_config
    }

    /// Attach a reader to the foreign DJ controller node at `addr`.
    ///
    /// Returns `None` if no node is currently registered at that address, if no
    /// DJ-class packets have been received from it yet, **or** if a view has
    /// already been taken for the same node — each node's triple-buffer output
    /// can only be claimed once, so the second call returns `None`.
    pub fn get_controller_view(&self, addr: SocketAddrV4) -> Option<DjControllerView> {
        self._runtime.block_on(async {
            let state = self.dispatcher.state.read().await;
            let ctrl = state.discovered_nodes.get(&addr)?.dj_controller.as_ref()?;
            Some(DjControllerView::new(
                ctrl.state.clone(),
                ctrl.request_tx.clone(),
            ))
        })
    }

    /// Convenience: return a [`DjControllerView`] for the first discovered node
    /// that has a DJ controller attached. Returns `None` if no such node has
    /// been seen yet.
    pub fn get_any_controller_view(&self) -> Option<DjControllerView> {
        self._runtime.block_on(async {
            let state = self.dispatcher.state.read().await;
            for node in state.discovered_nodes.values() {
                if let Some(ctrl) = node.dj_controller.as_ref() {
                    return Some(DjControllerView::new(
                        ctrl.state.clone(),
                        ctrl.request_tx.clone(),
                    ));
                }
            }
            None
        })
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
