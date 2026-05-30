//! Session layer (ARCHITECTURE.md §4).
//!
//! Owns the per-peer state map and the master-election FSM.  Single-
//! actor design: one task ([`SessionTask`]) holds the map outright;
//! every other layer talks to it via typed [`SessionCommand`] messages.
//! Read paths publish through [`arc_swap::ArcSwap`] snapshots — no
//! `Mutex` / `RwLock` anywhere in the crate.
//!
//! ## Sub-modules
//!
//! * [`auth`] — `PeerAuth` scaffolding for the (spec-unspecified)
//!   authentication handshake.
//! * [`peer`] — typed `Peer<Announcing|Active|Leaving>` state machine.
//! * [`election`] — master-election state machine.
//! * [`task`] — the actor itself.

pub mod auth;
pub mod command;
pub mod election;
pub mod peer;
pub mod snapshot;
pub mod task;

pub use auth::PeerAuth;
pub use command::SessionCommand;
pub use election::{Election, ElectionCandidate, ElectionState, ElectionWinner};
pub use peer::{LeaveReason, PEER_TIMEOUT, Peer, PeerActive, PeerAnnouncing, PeerLeaving};
pub use snapshot::{PeerStateKind, PeerSummary, SessionSnapshot};
pub use task::{DEFAULT_QUEUE_CAPACITY, SessionHandle, SessionTask};
