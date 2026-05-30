//! Hot-path runtime (ARCHITECTURE.md §7).
//!
//! Phase 7's deliverable is a three-thread layout for the RT hot
//! path:
//!
//! 1. **Recv thread** — drains the four UDP sockets and hands
//!    parsed datagrams to the session task via a lock-free queue.
//! 2. **Session thread** — runs the [`SessionTask`](crate::session)
//!    + domain writer, publishes the read-only snapshot.
//! 3. **Send thread** — drains per-channel outbound queues and
//!    calls `UdpSocket::send_to` synchronously.
//!
//! `tokio` stays alive for the cold-path user-facing async API
//! (snapshot reads, request/response).
//!
//! This commit lands the [`tick`] module — the drift-corrected
//! `Ticker` every RT thread uses for its cadence.  The thread-spawn
//! wiring lands together with the public-API rewrite in phase 8.
//!
//! ## Sub-modules
//!
//! * [`tick`] — `Ticker` with overrun detection and drift
//!   correction.

pub mod tick;

pub use tick::{TickStatus, Ticker};
