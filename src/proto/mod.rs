//! TCNet protocol behaviour.
//!
//! The [`time_sync`] module captures the three-step TimeSync handshake:
//! pending-reply tracking and signed clock-offset computation per spec
//! page 8. It is driven by the dispatcher's TimeSync initiator and
//! receive path.

pub mod time_sync;

pub use time_sync::{
    ClockOffset, DEFAULT_MAX_REPLY_AGE, PendingTimeSync, TimeSyncError, TimeSyncReply,
};
