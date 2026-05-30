use crate::into_ascii;
use crate::protocol::{AsciiString, NodeId, NodeOptions, NodeType};
use arc_swap::{ArcSwap, ArcSwapOption};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
pub(crate) mod dispatcher;
pub(crate) mod dj_controller;
pub(crate) mod response_data;
pub(crate) mod tcnet_packet;

use crate::node::dj_controller::DjController;

/// Per-peer record stored in [`DynamicNodeState`].
///
/// All mutable fields use interior mutability (`AtomicU64` /
/// `ArcSwap` / `ArcSwapOption`) so the containing
/// `Arc<ForeignNode>` is immutable to the caller — which in turn
/// lets the surrounding `HashMap<SocketAddrV4, Arc<ForeignNode>>`
/// live behind an `ArcSwap` (RCU on insert/remove only, no lock).
#[derive(Debug)]
pub(crate) struct ForeignNode {
    /// Wall-clock seconds since UNIX epoch of the most recent
    /// packet from this peer.  Atomically updated on every
    /// observed OptIn / DJ packet; read by the 1 Hz timeout sweep.
    pub last_seen: AtomicU64,
    /// Peer's listener address.  Reset on each OptIn in case the
    /// peer's port changes.
    pub address: ArcSwap<SocketAddrV4>,
    /// Peer's announced application config.  Republished on each
    /// OptIn.
    pub config: ArcSwap<ApplicationConfig>,
    /// Per-peer DJ-controller handle.  `None` until the first
    /// DJ-class packet (Status / Metrics / Time / Mixer / …) lands
    /// for this peer; set exactly once via
    /// [`ArcSwapOption::compare_and_swap`]-equivalent — subsequent
    /// updates to the same peer reuse the existing controller.
    pub dj_controller: ArcSwapOption<DjController>,
}

impl ForeignNode {
    pub fn new(address: SocketAddrV4, config: ApplicationConfig, last_seen: u64) -> Self {
        Self {
            last_seen: AtomicU64::new(last_seen),
            address: ArcSwap::from_pointee(address),
            config: ArcSwap::from_pointee(config),
            dj_controller: ArcSwapOption::empty(),
        }
    }

    pub fn last_seen(&self) -> u64 {
        self.last_seen.load(Ordering::Relaxed)
    }
    pub fn touch(&self, secs: u64) {
        self.last_seen.store(secs, Ordering::Relaxed);
    }
    pub fn address(&self) -> SocketAddrV4 {
        **self.address.load()
    }
    pub fn set_address(&self, addr: SocketAddrV4) {
        self.address.store(Arc::new(addr));
    }
    pub fn config(&self) -> ApplicationConfig {
        **self.config.load()
    }
    pub fn set_config(&self, cfg: ApplicationConfig) {
        self.config.store(Arc::new(cfg));
    }
    pub fn dj_controller(&self) -> Option<Arc<DjController>> {
        self.dj_controller.load_full()
    }
    /// Set the DJ controller (only the first call takes effect).
    /// Returns `true` if this call installed the controller.
    pub fn install_dj_controller(&self, ctrl: DjController) -> bool {
        // ArcSwapOption doesn't directly expose compare-and-swap, so
        // we approximate: check + store.  Two callers racing both
        // create their own controllers but only the second wins —
        // that's wasteful but safe.  The dispatcher serialises through
        // its own kanal queue so in practice there's only ever one
        // caller installing.
        if self.dj_controller.load().is_some() {
            return false;
        }
        self.dj_controller.store(Some(Arc::new(ctrl)));
        true
    }
    /// Snapshot helper for the published `ForeignNodeInfo` list.
    pub fn has_dj_controller(&self) -> bool {
        self.dj_controller.load().is_some()
    }
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
/// config.application_name = into_ascii!("myviz___________");
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
