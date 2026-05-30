//! Read-only [`SessionTask`](super::task::SessionTask) snapshot.
//!
//! `SessionSnapshot` is the wait-free read path into the session
//! layer.  The task publishes a fresh snapshot via
//! [`arc_swap::ArcSwap`] after every applied command; callers
//! `load_full()` a snapshot, walk it, drop the `Arc` when done.  No
//! lock, no allocation per-read.

use super::election::ElectionState;
use crate::ApplicationConfig;
use crate::protocol::NodeId;
use std::net::SocketAddrV4;
use std::time::Instant;

/// Which lifecycle bucket a peer is currently in.
///
/// Strips the data-carrying fields of [`Peer`](super::Peer); reads
/// only care about the tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerStateKind {
    Announcing,
    Active,
    Leaving,
}

/// One peer's summary — the read-side projection of
/// [`Peer`](super::Peer).
#[derive(Debug, Clone)]
pub struct PeerSummary {
    pub addr: SocketAddrV4,
    pub node_id: NodeId,
    pub state: PeerStateKind,
    pub last_seen: Instant,
    pub config: ApplicationConfig,
}

/// Read-only snapshot of the session task's state.
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    /// All currently-known peers.  Order is unspecified — sort if you
    /// need deterministic iteration.
    pub peers: Vec<PeerSummary>,
    /// Master-election state at snapshot time.
    pub election: ElectionState,
    /// Monotonically-increasing snapshot generation.  Lets consumers
    /// detect that a fresh snapshot has landed.
    pub generation: u64,
    /// When the task published this snapshot.
    pub published_at: Instant,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            peers: Vec::new(),
            election: ElectionState::default(),
            generation: 0,
            published_at: Instant::now(),
        }
    }
}

impl SessionSnapshot {
    /// Lookup a peer by address — `O(n)` but `n` is bounded by the
    /// number of TCNet peers on the LAN, which is small.  If you need
    /// `O(1)`, sort + binary-search by `addr` in the consumer.
    pub fn find(&self, addr: SocketAddrV4) -> Option<&PeerSummary> {
        self.peers.iter().find(|p| p.addr == addr)
    }

    /// Only Active peers — i.e. peers we've seen DJ traffic from.
    pub fn active_peers(&self) -> impl Iterator<Item = &PeerSummary> {
        self.peers
            .iter()
            .filter(|p| p.state == PeerStateKind::Active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn addr(last: u8) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, last), 65023)
    }

    fn summary(node_id: NodeId, addr: SocketAddrV4, state: PeerStateKind) -> PeerSummary {
        PeerSummary {
            addr,
            node_id,
            state,
            last_seen: Instant::now(),
            config: ApplicationConfig::default(),
        }
    }

    #[test]
    fn snapshot_default_is_empty() {
        let s = SessionSnapshot::default();
        assert_eq!(s.peers.len(), 0);
        assert_eq!(s.generation, 0);
    }

    #[test]
    fn snapshot_find_returns_matching_peer() {
        let s = SessionSnapshot {
            peers: vec![
                summary(1, addr(1), PeerStateKind::Active),
                summary(2, addr(2), PeerStateKind::Announcing),
            ],
            ..SessionSnapshot::default()
        };
        assert_eq!(s.find(addr(1)).unwrap().node_id, 1);
        assert_eq!(s.find(addr(2)).unwrap().node_id, 2);
        assert!(s.find(addr(3)).is_none());
    }

    #[test]
    fn snapshot_active_peers_filters_by_state() {
        let s = SessionSnapshot {
            peers: vec![
                summary(1, addr(1), PeerStateKind::Active),
                summary(2, addr(2), PeerStateKind::Announcing),
                summary(3, addr(3), PeerStateKind::Active),
                summary(4, addr(4), PeerStateKind::Leaving),
            ],
            ..SessionSnapshot::default()
        };
        let ids: Vec<NodeId> = s.active_peers().map(|p| p.node_id).collect();
        assert_eq!(ids, vec![1, 3]);
    }
}
