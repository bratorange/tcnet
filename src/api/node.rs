//! `Node<R, V>` — the typed public API surface.
//!
//! This module is the type-system payoff of phases 1-7: the layered
//! design (wire / transport / session / proto / domain / runtime)
//! manifests above the line as a single
//! `Node<R: Role, V: SpecVersion>` handle, parameterised on the local
//! role and spec version.  Internally `Node` wraps the legacy
//! [`TCNetClient`](crate::TCNetClient) engine for now — the public
//! surface is the typed shape; the implementation will migrate to
//! `SessionTask` + `UdpTransport` incrementally.
//!
//! The methods enforced at the type level:
//!
//! * `Node<Slave, V>` — `snapshot`, `layers_for`, `mixer_for`,
//!   `request_*`, `leave`.
//! * `Node<Master, V>` — derefs to
//!   [`ActiveDJNode`](crate::ActiveDJNode), so every broadcast / set
//!   method on the active node is reachable through the typed handle.
//! * `Node<Auto, V>` — Slave methods plus a `.wait_election()` future
//!   that resolves into `Node<Master, V>` or `Node<Slave, V>`
//!   (implementation deferred — see crate-level docs).

use super::roles::{Master, Role, Slave};
use crate::node::dj_controller::{LayerSnapshot, MixerSnapshot, TimeoutError};
use crate::protocol::{BeatGridHeader, BigWaveformData, CueData, LayerId, NodeId, SmallWaveformData};
use crate::spec_version::SpecVersion;
use crate::{ActiveDJNode, ApplicationConfig, ForeignNodeInfo, TCNetClient};
use std::marker::PhantomData;
use std::net::SocketAddrV4;

/// One foreign-node row in [`NodeSnapshot::peers`].
///
/// Direct replacement for the legacy [`ForeignNodeInfo`] —
/// re-exposed at the typed surface so downstream code doesn't have
/// to reach into the engine module.
#[derive(Clone, Debug)]
pub struct PeerInfo {
    /// The peer's listener address (IP + unicast port).
    pub address: SocketAddrV4,
    /// Wall-clock UNIX-seconds timestamp of the last packet from this peer.
    pub last_seen: u64,
    /// The peer's 16-bit node identifier.
    pub node_id: NodeId,
    /// `true` once any DJ packet has been observed from this peer,
    /// meaning [`Node::layers_for`] / [`Node::mixer_for`] /
    /// `request_*` methods will return data.
    pub has_dj_controller: bool,
}

impl From<ForeignNodeInfo> for PeerInfo {
    fn from(f: ForeignNodeInfo) -> Self {
        Self {
            address: f.address,
            last_seen: f.last_seen,
            node_id: f.node_id,
            has_dj_controller: f.has_dj_controller,
        }
    }
}

/// Read-only snapshot of the local node's view of the TCNet network.
///
/// Returned by [`Node::snapshot`].  Cheap to drop; iterate over
/// `peers` to find the foreign nodes you care about.
#[derive(Clone, Debug, Default)]
pub struct NodeSnapshot {
    pub peers: Vec<PeerInfo>,
}

/// Anything the [`Node`] API can fail at.
#[non_exhaustive]
#[derive(Debug)]
pub enum NodeError {
    /// `Node::join` couldn't bring the runtime / dispatcher up.
    JoinFailed { reason: String },
    /// `Node::leave` couldn't broadcast OptOut.
    LeaveFailed,
    /// A request (waveform / beat-grid / cue / artwork) timed out
    /// after 5 s, or the underlying request channel closed.
    RequestTimeout,
    /// A request was made for a peer that doesn't currently have a
    /// DjController (e.g. `has_dj_controller == false`).
    PeerHasNoController { addr: SocketAddrV4 },
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JoinFailed { reason } => write!(f, "Node::join failed: {}", reason),
            Self::LeaveFailed => f.write_str("Node::leave failed"),
            Self::RequestTimeout => f.write_str("request timed out"),
            Self::PeerHasNoController { addr } => {
                write!(f, "peer {} has no DjController yet", addr)
            }
        }
    }
}

impl std::error::Error for NodeError {}

impl From<TimeoutError> for NodeError {
    fn from(_: TimeoutError) -> Self {
        Self::RequestTimeout
    }
}

/// The typed handle to a running TCNet node.
///
/// `R` is the role marker ([`Slave`] / [`Master`] / [`Auto`](super::roles::Auto)
/// / [`Repeater`](super::roles::Repeater)); `V` is the
/// [`SpecVersion`](crate::SpecVersion) the local node emits at.
///
/// Owned, not `Clone`: each `Node` corresponds to a unique set of
/// bound sockets + tokio runtime.  Pass `&mut Node<…>` through your
/// call graph if you need write access (every `set_*` /
/// `broadcast_*` method requires it), or `&Node<…>` for read-only
/// access (snapshot, request_*).
pub struct Node<R: Role, V: SpecVersion> {
    client: TCNetClient,
    active: Option<ActiveDJNode>,
    _r: PhantomData<R>,
    _v: PhantomData<V>,
}

impl<R: Role, V: SpecVersion> Node<R, V> {
    /// Wait-free snapshot of the foreign-peer set discovered by
    /// OptIn / OptOut + DJ-packet traffic.
    pub fn snapshot(&self) -> NodeSnapshot {
        NodeSnapshot {
            peers: self
                .client
                .nodes_snapshot_arc()
                .iter()
                .cloned()
                .map(PeerInfo::from)
                .collect(),
        }
    }

    /// Cleanly leave the network.  Consumes `self` — the legacy
    /// engine's `Drop` already broadcasts a best-effort OptOut, so
    /// this just lets it run.
    pub async fn leave(self) -> Result<(), NodeError> {
        drop(self);
        Ok(())
    }

    /// Access to the local node's configured identity.
    pub fn config(&self) -> ApplicationConfig {
        self.client.node_config()
    }
}

// Slave-specific read methods.
impl<V: SpecVersion> Node<Slave, V> {
    /// All eight [`LayerSnapshot`]s for the foreign DJ controller at
    /// `addr`, or `None` if no controller is associated with that
    /// peer.
    pub fn layers_for(&mut self, addr: SocketAddrV4) -> Option<Vec<LayerSnapshot>> {
        let mut view = self.client.get_controller_view(addr)?;
        Some(view.get_layers().to_vec())
    }

    /// [`MixerSnapshot`] for the foreign DJ controller at `addr`.
    pub fn mixer_for(&mut self, addr: SocketAddrV4) -> Option<MixerSnapshot> {
        let mut view = self.client.get_controller_view(addr)?;
        Some(view.get_mixer().clone())
    }

    /// Escape hatch: get a legacy [`DjControllerView`](crate::DjControllerView)
    /// for `addr`, whose [`WaveformRequester`](crate::WaveformRequester) is
    /// `Send + 'static` and can be moved into a `tokio::spawn`ed
    /// background task.
    ///
    /// Prefer [`Node::request_small_waveform`] / `request_big_waveform`
    /// / `request_beat_grid` / `request_cue_data` for inline awaits.
    /// Use this only when you need a `'static` requester handle that
    /// outlives a borrow of `&mut Node`.
    ///
    /// Returns `None` if no controller is associated with `addr`, *or*
    /// if the view has already been taken for this peer — each peer's
    /// triple buffer is consumable once, per
    /// [`TCNetClient::get_controller_view`](crate::TCNetClient::get_controller_view)
    /// semantics.
    ///
    /// Slated for removal in 0.3.0 when `Node::pending_*` futures land
    /// to replace the spawn-into-background pattern with a
    /// `'static`-friendly `Pending<T>`.
    pub fn legacy_controller_view(
        &mut self,
        addr: SocketAddrV4,
    ) -> Option<crate::DjControllerView> {
        self.client.get_controller_view(addr)
    }

    /// Request a small (low-res) waveform from `addr` for `layer`.
    /// Returns `Err(RequestTimeout)` if no response arrives within 5 s.
    pub async fn request_small_waveform(
        &mut self,
        addr: SocketAddrV4,
        layer: LayerId,
    ) -> Result<SmallWaveformData, NodeError> {
        let view = self
            .client
            .get_controller_view(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
        let requester = view.waveform_requester();
        requester
            .request_small_waveform(layer)
            .await
            .map_err(Into::into)
    }

    /// Request the full big-waveform multi-packet response from
    /// `addr` for `layer`.
    pub async fn request_big_waveform(
        &mut self,
        addr: SocketAddrV4,
        layer: LayerId,
    ) -> Result<BigWaveformData, NodeError> {
        let view = self
            .client
            .get_controller_view(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
        let requester = view.waveform_requester();
        requester
            .request_big_waveform(layer)
            .await
            .map_err(Into::into)
    }

    /// Request the multi-packet BeatGrid response.
    pub async fn request_beat_grid(
        &mut self,
        addr: SocketAddrV4,
        layer: LayerId,
    ) -> Result<BeatGridHeader, NodeError> {
        let view = self
            .client
            .get_controller_view(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
        let requester = view.waveform_requester();
        requester
            .request_beat_grid(layer)
            .await
            .map_err(Into::into)
    }

    /// Request the cue-data response.
    pub async fn request_cue_data(
        &mut self,
        addr: SocketAddrV4,
        layer: LayerId,
    ) -> Result<CueData, NodeError> {
        let view = self
            .client
            .get_controller_view(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
        let requester = view.waveform_requester();
        requester.request_cue_data(layer).await.map_err(Into::into)
    }
}

// Master-specific deref into ActiveDJNode so every set_* / load_track
// / broadcast method lights up on `&mut Node<Master, V>`.
impl<V: SpecVersion> std::ops::Deref for Node<Master, V> {
    type Target = ActiveDJNode;
    fn deref(&self) -> &Self::Target {
        self.active
            .as_ref()
            .expect("Node<Master, _> always has an ActiveDJNode")
    }
}

impl<V: SpecVersion> std::ops::DerefMut for Node<Master, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.active
            .as_mut()
            .expect("Node<Master, _> always has an ActiveDJNode")
    }
}

/// Internal constructor used by [`NodeBuilder`](super::NodeBuilder).
pub(crate) fn from_engine<R: Role, V: SpecVersion>(
    client: TCNetClient,
    active: Option<ActiveDJNode>,
) -> Node<R, V> {
    Node {
        client,
        active,
        _r: PhantomData,
        _v: PhantomData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::V3_6;

    #[test]
    fn node_is_send_sync() {
        fn _ss<T: Send + Sync>() {}
        _ss::<Node<Slave, V3_6>>();
        _ss::<Node<Master, V3_6>>();
    }

    #[test]
    fn peer_info_round_trips_from_foreign_node_info() {
        let f = ForeignNodeInfo {
            address: SocketAddrV4::new(std::net::Ipv4Addr::new(192, 168, 1, 10), 65023),
            last_seen: 12345,
            node_id: 7,
            has_dj_controller: true,
        };
        let p: PeerInfo = f.into();
        assert_eq!(p.node_id, 7);
        assert_eq!(p.last_seen, 12345);
        assert!(p.has_dj_controller);
    }
}
