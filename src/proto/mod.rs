//! TCNet protocol machines (ARCHITECTURE.md §5).
//!
//! Where the [`wire`](crate::protocol) layer parses bytes into typed
//! structs, the protocol layer captures *behaviour*: chunk
//! reassembly, the TimeSync three-step handshake, typed control
//! paths, AppSpecific routing, request/response pending state.  Each
//! sub-module is one self-contained state machine.
//!
//! ## Sub-modules
//!
//! * [`chunked`] — generic multi-packet response reassembly
//!   (`ChunkedFrame<T>` replaces the three copy-pasted accumulators
//!   in `dj_controller.rs`).
//! * `time_sync` — three-step TimeSync handshake (phase 5.2).
//! * `control` / `text` / `keyboard` — typed message builders
//!   (phase 5.3 / 5.4).
//! * `app_specific` — AppSpecific 30 + 213 reassembly (phase 5.5).
//! * `request` — typed `Pending<T>` futures (phase 5.6).
//! * `error_notif` — `ErrorNotification` routing (phase 5.7).

pub mod chunked;
pub mod time_sync;

pub use chunked::{AcceptOutcome, ChunkedFrame, ChunkedPayload, MismatchReason};
pub use time_sync::{
    ClockOffset, DEFAULT_MAX_REPLY_AGE, PendingTimeSync, TimeSyncError, TimeSyncReply,
};
