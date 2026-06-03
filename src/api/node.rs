//! `Node<R, V>` — the typed public API surface.
//!
//! A single `Node<R: Role, V: SpecVersion>` handle over the dispatcher
//! runtime, parameterised on the local role and spec version.
//!
//! What the role marker `R` gates:
//!
//! * **Read methods are available on every role** —
//!   [`snapshot`](Node::snapshot), [`layers_for`](Node::layers_for),
//!   [`mixer_for`](Node::mixer_for), `request_*`,
//!   [`clock_offset_for`](Node::clock_offset_for),
//!   [`election_state`](Node::election_state), [`leave`](Node::leave).
//!   Watching foreign controllers is orthogonal to broadcasting.
//! * `Node<Master, V>` additionally `Deref`s to the broadcaster handle
//!   ([`ActiveDJNode`]), lighting up the write surface: `set_speed`,
//!   `set_bpm`, `set_layer_position`, `set_cue_marker`, `set_hot_cues`,
//!   `load_track`, `set_master_fader`, `set_crossfader`,
//!   `set_channel_*`, `set_response_*`, ...
//! * `Node<Slave, V>` / `Node<Auto, V>` / `Node<Repeater, V>` carry no
//!   broadcaster, so only the read surface is reachable. The marker
//!   still drives the `NodeType` this node announces on the wire (so
//!   `Auto` is electable, `Slave` is not).

use super::roles::{Master, Role};
use crate::node::dj_controller::{LayerSnapshot, MixerSnapshot, TimeoutError};
use crate::protocol::{
    ArtworkFileData, BeatGridHeader, BigWaveformData, CueData, LayerId, NodeId, SmallWaveformData,
};
use crate::spec_version::SpecVersion;
use crate::{ActiveDJNode, ApplicationConfig, DjControllerView, ForeignNodeInfo, TCNetClient, WaveformRequester};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::net::SocketAddrV4;

/// One foreign-node row in [`NodeSnapshot::peers`].
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
    /// [`NodeBuilder::spawn`](crate::api::NodeBuilder::spawn) couldn't
    /// bring the runtime / dispatcher up.
    SpawnFailed { reason: String },
    /// A request (waveform / beat-grid / cue) did not resolve. Raised
    /// when the 5 s deadline elapses or the request channel closed. A
    /// peer that has no data answers with `ErrorNotification(014, EMPTY)`,
    /// which is not parsed on the requesting side, so an empty peer also
    /// surfaces here once the deadline passes.
    RequestTimeout,
    /// A request was made for a peer that doesn't currently have a
    /// DjController (e.g. `has_dj_controller == false`).
    PeerHasNoController { addr: SocketAddrV4 },
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpawnFailed { reason } => write!(f, "NodeBuilder::spawn failed: {}", reason),
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
/// `R` is the role marker ([`Slave`](super::roles::Slave) /
/// [`Master`] / [`Auto`](super::roles::Auto) /
/// [`Repeater`](super::roles::Repeater)), which selects the announced
/// `NodeType` and gates the write surface. `V` is the declared
/// [`SpecVersion`]; see that module for what the
/// marker does and does not gate today.
///
/// Owned, not `Clone`: each `Node` corresponds to a unique set of
/// bound sockets + tokio runtime.  Pass `&mut Node<…>` through your
/// call graph if you need write access (every `set_*` /
/// `broadcast_*` method requires it), or `&Node<…>` for read-only
/// access (snapshot, request_*).
pub struct Node<R: Role, V: SpecVersion> {
    client: TCNetClient,
    active: Option<ActiveDJNode>,
    /// Per-peer view cache.  Reads are lock-free via the per-peer
    /// `Arc<ArcSwap<DjControllerState>>` — claiming a view here just
    /// clones the `Arc`; we keep it around for the lifetime of the
    /// `Node` so `layers_for` / `mixer_for` / `request_*` calls on
    /// the same address share state.
    views: HashMap<SocketAddrV4, DjControllerView>,
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

    /// Cleanly leave the network.  Consumes `self` — `Drop` broadcasts
    /// a best-effort OptOut packet to every discovered peer + the
    /// loopback fallback before the runtime tears down.
    pub async fn leave(self) -> Result<(), NodeError> {
        drop(self);
        Ok(())
    }

    /// Access to the local node's configured identity.
    pub fn config(&self) -> ApplicationConfig {
        self.client.node_config()
    }

    /// Most recent successful TimeSync result for `peer`, or `None`.
    /// See [`crate::proto::ClockOffset`].
    pub fn clock_offset_for(
        &self,
        peer: SocketAddrV4,
    ) -> Option<crate::proto::ClockOffset> {
        self.client.clock_offset_for(peer)
    }

    /// Current master-election state.  See
    /// [`crate::session::ElectionState`].
    pub fn election_state(&self) -> crate::session::ElectionState {
        self.client.election_state()
    }

    /// Handle to the internal tokio runtime so callers can spawn
    /// supplementary async work (e.g. background waveform / cue
    /// pullers) without standing up a second runtime.
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.client.runtime_handle()
    }
}

// Foreign-peer read methods — available on every role. Reading a
// peer's layers / mixer / waveforms is orthogonal to whether the local
// node broadcasts, so a `Master` can watch other controllers too.
impl<R: Role, V: SpecVersion> Node<R, V> {
    /// Lazily claim (and cache) the per-peer view for `addr`.
    /// Returns `None` if no DJ-controller-bearing peer is registered
    /// at that address yet.
    fn view_for_mut(&mut self, addr: SocketAddrV4) -> Option<&mut DjControllerView> {
        use std::collections::hash_map::Entry;
        match self.views.entry(addr) {
            Entry::Occupied(e) => Some(e.into_mut()),
            Entry::Vacant(e) => {
                let view = self.client.get_controller_view(addr)?;
                Some(e.insert(view))
            }
        }
    }

    /// All eight [`LayerSnapshot`]s for the foreign DJ controller at
    /// `addr`, or `None` if no controller is associated with that
    /// peer.
    pub fn layers_for(&mut self, addr: SocketAddrV4) -> Option<Vec<LayerSnapshot>> {
        let view = self.view_for_mut(addr)?;
        Some(view.get_layers().to_vec())
    }

    /// [`MixerSnapshot`] for the foreign DJ controller at `addr`.
    pub fn mixer_for(&mut self, addr: SocketAddrV4) -> Option<MixerSnapshot> {
        let view = self.view_for_mut(addr)?;
        Some(view.get_mixer().clone())
    }

    /// A `Send + 'static` [`WaveformRequester`] for `addr` that can be
    /// moved into a `tokio::spawn`ed background task — for long-lived
    /// pullers that need to outlive a borrow of `&mut Node`.
    ///
    /// Prefer [`Node::request_small_waveform`] / `request_big_waveform`
    /// / `request_beat_grid` / `request_cue_data` for inline awaits.
    /// Returns `None` if no controller is associated with `addr` yet.
    pub fn waveform_requester_for(&mut self, addr: SocketAddrV4) -> Option<WaveformRequester> {
        let view = self.view_for_mut(addr)?;
        Some(view.waveform_requester())
    }

    /// Request a small (low-res) waveform from `addr` for `layer`.
    ///
    /// `Err(`[`PeerHasNoController`](NodeError::PeerHasNoController)`)` if the
    /// peer hasn't sent a DJ packet yet; `Err(`[`RequestTimeout`](NodeError::RequestTimeout)`)`
    /// if the reply doesn't arrive within 5 s.
    pub async fn request_small_waveform(
        &mut self,
        addr: SocketAddrV4,
        layer: LayerId,
    ) -> Result<SmallWaveformData, NodeError> {
        let requester = self
            .waveform_requester_for(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
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
        let requester = self
            .waveform_requester_for(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
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
        let requester = self
            .waveform_requester_for(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
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
        let requester = self
            .waveform_requester_for(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
        requester.request_cue_data(layer).await.map_err(Into::into)
    }

    /// Request the low-resolution artwork JPEG from `addr` for `layer`.
    pub async fn request_artwork_file(
        &mut self,
        addr: SocketAddrV4,
        layer: LayerId,
    ) -> Result<ArtworkFileData, NodeError> {
        let requester = self
            .waveform_requester_for(addr)
            .ok_or(NodeError::PeerHasNoController { addr })?;
        requester
            .request_artwork_file(layer)
            .await
            .map_err(Into::into)
    }
}

// Master-specific deref into the broadcaster handle so every set_* /
// load_track / broadcast method lights up on `&mut Node<Master, V>`.
impl<V: SpecVersion> std::ops::Deref for Node<Master, V> {
    type Target = ActiveDJNode;
    fn deref(&self) -> &Self::Target {
        self.active
            .as_ref()
            .expect("Node<Master, _> always has a broadcaster")
    }
}

impl<V: SpecVersion> std::ops::DerefMut for Node<Master, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.active
            .as_mut()
            .expect("Node<Master, _> always has a broadcaster")
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
        views: HashMap::new(),
        _r: PhantomData,
        _v: PhantomData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::roles::Slave;
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
