//! Typed per-peer state machine (ARCHITECTURE.md §4.2).
//!
//! A foreign node goes through three states:
//!
//! ```text
//!   ┌──────────────┐  first DJ packet   ┌──────────────┐  silence ≥10 s    ┌──────────────┐
//!   │ Announcing   │ ──────────────────► │  Active      │ ────────────────► │  Leaving     │
//!   └──────┬───────┘                     └──────┬───────┘    OR OptOut      └──────────────┘
//!          │                                    │
//!          │  silence ≥10 s (never promoted)    │
//!          └──────────────────────────────────► Leaving
//! ```
//!
//! - `Announcing` — we've seen an OptIn but no DJ packets (Status,
//!   Time, Metrics, …). We know the peer exists but not yet what
//!   layers it carries.
//! - `Active` — we've seen at least one DJ packet, so we have a
//!   `DjController`-shaped snapshot for it. Timestamp the most recent
//!   packet for the heartbeat-timeout check.
//! - `Leaving` — the peer sent OptOut, or we haven't heard from them
//!   in the spec-mandated 10-second window. The session task will
//!   evict it on the next sweep.
//!
//! Each variant exposes only the methods that make sense in its
//! state.  Transitions consume `self` so a stale handle to an old
//! state is a compile-time error.

use super::auth::PeerAuth;
use crate::ApplicationConfig;
use crate::protocol::NodeId;
use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

/// Spec-mandated silence threshold before a peer is presumed gone.
pub const PEER_TIMEOUT: Duration = Duration::from_secs(10);

/// A peer at one of three lifecycle stages.
///
/// Owned exclusively by the [`SessionTask`](super::SessionTask); read
/// access happens via the published `SessionSnapshot`.
#[derive(Debug, Clone)]
pub enum Peer {
    Announcing(PeerAnnouncing),
    Active(PeerActive),
    Leaving(PeerLeaving),
}

impl Peer {
    /// Common: the peer's address.
    pub fn addr(&self) -> SocketAddrV4 {
        match self {
            Self::Announcing(p) => p.addr,
            Self::Active(p) => p.addr,
            Self::Leaving(p) => p.addr,
        }
    }

    /// Common: the peer's node id.
    pub fn node_id(&self) -> NodeId {
        match self {
            Self::Announcing(p) => p.node_id,
            Self::Active(p) => p.node_id,
            Self::Leaving(p) => p.node_id,
        }
    }

    /// Common: most-recent activity timestamp.
    pub fn last_seen(&self) -> Instant {
        match self {
            Self::Announcing(p) => p.announced_at,
            Self::Active(p) => p.last_seen,
            Self::Leaving(p) => p.left_at,
        }
    }

    /// Has the peer been silent past [`PEER_TIMEOUT`]?
    pub fn is_timed_out(&self, now: Instant) -> bool {
        now.duration_since(self.last_seen()) >= PEER_TIMEOUT
    }
}

/// Peer with an OptIn but no DJ traffic yet.
#[derive(Debug, Clone)]
pub struct PeerAnnouncing {
    pub addr: SocketAddrV4,
    pub node_id: NodeId,
    pub config: ApplicationConfig,
    pub announced_at: Instant,
    pub auth: PeerAuth,
}

impl PeerAnnouncing {
    pub fn new(
        addr: SocketAddrV4,
        node_id: NodeId,
        config: ApplicationConfig,
        announced_at: Instant,
    ) -> Self {
        Self {
            addr,
            node_id,
            config,
            announced_at,
            auth: PeerAuth::Anonymous,
        }
    }

    /// We saw a DJ packet (Status / Time / Metrics / Meta / Mixer)
    /// from this peer — promote to `Active`.
    pub fn promote(self, dj_packet_at: Instant) -> PeerActive {
        PeerActive {
            addr: self.addr,
            node_id: self.node_id,
            config: self.config,
            last_seen: dj_packet_at,
            auth: self.auth,
        }
    }

    /// Time's up — never heard a DJ packet.
    pub fn timeout(self, at: Instant) -> PeerLeaving {
        PeerLeaving {
            addr: self.addr,
            node_id: self.node_id,
            left_at: at,
            reason: LeaveReason::Timeout,
        }
    }

    /// Peer sent OptOut without ever being promoted.
    pub fn opt_out(self, at: Instant) -> PeerLeaving {
        PeerLeaving {
            addr: self.addr,
            node_id: self.node_id,
            left_at: at,
            reason: LeaveReason::OptOut,
        }
    }
}

/// Peer that has DJ-controller traffic flowing.
///
/// `last_seen` is updated by [`PeerActive::touch`] on every packet
/// from this peer, so the timeout test stays correct.
#[derive(Debug, Clone)]
pub struct PeerActive {
    pub addr: SocketAddrV4,
    pub node_id: NodeId,
    pub config: ApplicationConfig,
    pub last_seen: Instant,
    pub auth: PeerAuth,
}

impl PeerActive {
    /// Stamp a fresh packet's arrival time.
    pub fn touch(&mut self, at: Instant) {
        self.last_seen = at;
    }

    /// Peer sent OptOut.
    pub fn opt_out(self, at: Instant) -> PeerLeaving {
        PeerLeaving {
            addr: self.addr,
            node_id: self.node_id,
            left_at: at,
            reason: LeaveReason::OptOut,
        }
    }

    /// Heartbeat silence elapsed.
    pub fn timeout(self, at: Instant) -> PeerLeaving {
        PeerLeaving {
            addr: self.addr,
            node_id: self.node_id,
            left_at: at,
            reason: LeaveReason::Timeout,
        }
    }
}

/// Why a peer left — distinguishing OptOut from silence helps the
/// election state machine in [`super::election`] decide whether to
/// re-run the auction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveReason {
    /// Peer explicitly sent an OptOut packet.
    OptOut,
    /// We've gone past `PEER_TIMEOUT` of silence.
    Timeout,
}

/// Peer that's leaving — pending eviction from the session map.
#[derive(Debug, Clone)]
pub struct PeerLeaving {
    pub addr: SocketAddrV4,
    pub node_id: NodeId,
    pub left_at: Instant,
    pub reason: LeaveReason,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn addr(last: u8) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, last), 65023)
    }

    fn cfg() -> ApplicationConfig {
        ApplicationConfig::default()
    }

    #[test]
    fn announcing_promotes_to_active_via_consume() {
        let t0 = Instant::now();
        let announcing = PeerAnnouncing::new(addr(10), 42, cfg(), t0);
        let active = announcing.promote(t0 + Duration::from_millis(100));
        assert_eq!(active.node_id, 42);
        assert_eq!(active.addr, addr(10));
        assert_eq!(active.last_seen, t0 + Duration::from_millis(100));
    }

    #[test]
    fn announcing_timeout_lands_in_leaving_with_reason_timeout() {
        let t0 = Instant::now();
        let announcing = PeerAnnouncing::new(addr(10), 42, cfg(), t0);
        let leaving = announcing.timeout(t0 + PEER_TIMEOUT);
        assert_eq!(leaving.reason, LeaveReason::Timeout);
        assert_eq!(leaving.left_at, t0 + PEER_TIMEOUT);
    }

    #[test]
    fn announcing_opt_out_lands_in_leaving_with_reason_optout() {
        let t0 = Instant::now();
        let announcing = PeerAnnouncing::new(addr(10), 42, cfg(), t0);
        let leaving = announcing.opt_out(t0 + Duration::from_secs(1));
        assert_eq!(leaving.reason, LeaveReason::OptOut);
    }

    #[test]
    fn active_touch_updates_last_seen() {
        let t0 = Instant::now();
        let mut active = PeerAnnouncing::new(addr(10), 42, cfg(), t0).promote(t0);
        let later = t0 + Duration::from_secs(3);
        active.touch(later);
        assert_eq!(active.last_seen, later);
    }

    #[test]
    fn active_timeout_lands_in_leaving_with_reason_timeout() {
        let t0 = Instant::now();
        let active = PeerAnnouncing::new(addr(10), 42, cfg(), t0).promote(t0);
        let leaving = active.timeout(t0 + PEER_TIMEOUT);
        assert_eq!(leaving.reason, LeaveReason::Timeout);
    }

    #[test]
    fn peer_is_timed_out_after_threshold() {
        let t0 = Instant::now();
        let active = Peer::Active(PeerAnnouncing::new(addr(10), 42, cfg(), t0).promote(t0));
        assert!(!active.is_timed_out(t0 + Duration::from_secs(5)));
        assert!(active.is_timed_out(t0 + PEER_TIMEOUT));
    }

    #[test]
    fn peer_common_accessors_dispatch_per_variant() {
        let t0 = Instant::now();
        let announcing = PeerAnnouncing::new(addr(1), 11, cfg(), t0);
        let p_a = Peer::Announcing(announcing.clone());
        assert_eq!(p_a.node_id(), 11);
        assert_eq!(p_a.addr(), addr(1));

        let active = announcing.promote(t0);
        let p_act = Peer::Active(active.clone());
        assert_eq!(p_act.node_id(), 11);

        let leaving = active.opt_out(t0);
        let p_lv = Peer::Leaving(leaving);
        assert_eq!(p_lv.node_id(), 11);
    }
}
