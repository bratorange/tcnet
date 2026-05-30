//! Typed messages into the [`SessionTask`](super::task::SessionTask).
//!
//! All session-state mutation funnels through one of these commands.
//! The single-actor design means the task never needs a `Mutex` on
//! the peer map — it owns the map outright, drains its command queue,
//! mutates, publishes a fresh snapshot.
//!
//! Producers (UDP recv tasks, user-facing API, periodic ticker) clone
//! a [`SessionHandle`](super::task::SessionHandle) and call its typed
//! `observe_*` / `tick` methods; the handle wraps the actual queue
//! send.

use crate::ApplicationConfig;
use crate::protocol::NodeId;
use std::net::SocketAddrV4;
use std::time::Instant;

/// A single mutation request for the session task.
#[derive(Debug, Clone)]
pub enum SessionCommand {
    /// An OptIn packet arrived.  Upsert as `Announcing` if new, or
    /// refresh `config` if known.
    ObserveOptIn {
        src: SocketAddrV4,
        node_id: NodeId,
        /// Peer's announced config (vendor, app name, options, …).
        config: ApplicationConfig,
        /// Peer's announced uptime, used by election tie-break.
        uptime_secs: u32,
        /// Whether the peer announced as `NodeType::Master` /
        /// `NodeType::Auto` — drives election candidacy.
        claims_master: bool,
        at: Instant,
    },

    /// A DJ packet (Status / Time / Metrics / Meta / Mixer) arrived.
    /// Promotes `Announcing` peers to `Active` and refreshes
    /// `last_seen` on already-`Active` peers.  Silently dropped if
    /// the peer is unknown — we'd have seen OptIn first.
    ObserveDjPacket {
        src: SocketAddrV4,
        at: Instant,
    },

    /// An OptOut packet arrived.  Move the peer to `Leaving`; the
    /// next tick evicts it.
    ObserveOptOut {
        src: SocketAddrV4,
        at: Instant,
    },

    /// Periodic heartbeat — evict timed-out peers and re-resolve the
    /// election.  Should fire ~1 Hz.
    Tick { now: Instant },

    /// Shutdown the task on the next drain.  Producers should drop
    /// their `SessionHandle` after sending this.
    Shutdown,
}
