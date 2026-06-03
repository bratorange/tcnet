//! Session layer: master-election state machine.
//!
//! The [`Election`] FSM ranks candidate master nodes and is driven by
//! the dispatcher's `election_driver`. Read paths publish through
//! [`arc_swap::ArcSwap`] snapshots — no `Mutex` / `RwLock` anywhere.

pub mod election;

pub use election::{Election, ElectionCandidate, ElectionState, ElectionWinner};
