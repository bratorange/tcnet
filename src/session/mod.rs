//! Session layer (ARCHITECTURE.md §4).
//!
//! The session layer owns the per-peer state map.  It's a single-actor
//! design: one task (the [`SessionTask`], phase 4.3) holds the map
//! outright; every other layer talks to it via typed command messages.
//! Read paths publish through [`arc_swap::ArcSwap`] snapshots so the
//! rest of the crate never blocks on a `Mutex`.
//!
//! This is the layer that retires the last
//! `Arc<tokio::sync::RwLock<DynamicNodeState>>` in the legacy
//! `src/node/dispatcher.rs`.
//!
//! ## Sub-modules
//!
//! * [`auth`] — `PeerAuth` enum scaffolding for the (currently
//!   unspecified) authentication handshake.
//! * [`peer`] — typed `Peer<Announcing|Active|Leaving>` state machine.
//! * [`election`] — master-election state machine (phase 4.4).
//! * [`task`] — the actor itself (phase 4.3).

pub mod auth;
pub mod peer;

pub use auth::PeerAuth;
pub use peer::{LeaveReason, PEER_TIMEOUT, Peer, PeerActive, PeerAnnouncing, PeerLeaving};
