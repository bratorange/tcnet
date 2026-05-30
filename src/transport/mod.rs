//! Transport-layer abstraction (ARCHITECTURE.md §3).
//!
//! The transport hides every UDP-socket detail from the rest of the
//! crate. Above it, the session / protocol / domain layers see four
//! named [`Channel`]s and exchange `(SocketAddrV4, &[u8])`; the
//! underlying sockets, queues, buffer pools, and overflow policies all
//! live inside [`Transport`] impls.
//!
//! Two impls land in phase 3:
//!
//! * [`UdpTransport`](udp) — wraps tokio's `UdpSocket`s.  Phase 7
//!   replaces it with three dedicated OS threads using
//!   `clock_nanosleep` ticking; the trait stays the same.
//! * [`MemoryTransport`](memory) — programmable in-process loopback
//!   for tests.  Lets the session-layer property tests run without
//!   binding real ports (which is also why our CI does not hit
//!   `60000-60002/65023`).
//!
//! The trait is sync.  The cold-path user-facing API (phase 8) wraps
//! channel sends in `async fn`s; the hot path (recv → session →
//! send) talks to the trait directly from an `std::thread`.

pub mod channel;
pub mod error;

pub use channel::{Channel, ChannelConfig, ChannelStatus, OverflowPolicy};
pub use error::TransportError;

use std::net::SocketAddrV4;

/// One inbound UDP datagram, borrowed out of a transport's recv buffer.
///
/// The lifetime `'b` ties `bytes` to the caller-provided scratch buffer
/// — the transport doesn't allocate per-packet.
#[derive(Debug)]
pub struct IncomingDatagram<'b> {
    /// The raw packet bytes, exactly as received on the wire.
    pub bytes: &'b [u8],
    /// The peer that sent the datagram.
    pub src: SocketAddrV4,
    /// Which named channel (i.e. which of our sockets) received it.
    pub channel: Channel,
}

/// Lower-half wire interface.
///
/// Every implementor is `Send + Sync + 'static` so it can live behind
/// an `Arc` shared across the recv / session / send threads.
///
/// # Lifetime model
///
/// The recv-side API borrows from a caller-owned `&mut [u8]` so the
/// transport never allocates per-packet.  The buffer pool (phase 3.2)
/// uses fixed-size 8 192-byte slots; callers that want zero-copy
/// pipelines can park a pool slot in [`IncomingDatagram::bytes`] for
/// the duration of the parse.
///
/// # Send semantics
///
/// `send` is synchronous and returns immediately.  Whether the
/// datagram makes it onto the wire depends on the [`OverflowPolicy`]
/// configured for the [`Channel`]; check [`Transport::channel_status`]
/// for cumulative drop counts.
pub trait Transport: Send + Sync + 'static {
    /// Concrete error type returned by this transport's fallible
    /// methods.  `UdpTransport` returns [`TransportError`];
    /// `MemoryTransport` returns its own scripted error for fault
    /// injection.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Hand `bytes` off to the channel's send queue, addressed to
    /// `dest`.  Returns `Ok(())` as soon as the bytes are queued; the
    /// actual `sendto(2)` happens asynchronously.
    ///
    /// Errors only signal *non-recoverable* problems (channel closed,
    /// payload over MTU).  Queue overflow is handled by
    /// [`OverflowPolicy`] and surfaces in [`ChannelStatus::dropped`],
    /// not as an error.
    fn send(
        &self,
        channel: Channel,
        dest: SocketAddrV4,
        bytes: &[u8],
    ) -> Result<(), Self::Error>;

    /// Non-blocking attempt to receive one datagram into `buf`.
    /// Returns `None` if no datagram is ready.
    ///
    /// The returned `IncomingDatagram` borrows from `buf` for its
    /// lifetime; the caller may keep it until the parse completes,
    /// then call `try_recv` again to overwrite.
    fn try_recv<'b>(&self, buf: &'b mut [u8]) -> Option<IncomingDatagram<'b>>;

    /// Cheap atomic snapshot of the channel's queue health.  Reads
    /// don't lock — implementors publish via
    /// [`arc_swap::ArcSwap`](https://docs.rs/arc-swap) under the hood.
    fn channel_status(&self, channel: Channel) -> ChannelStatus;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Transport` is object-safe.  Phase 4's `SessionTask` will hold
    /// a `dyn Transport<Error = TransportError>` so it can be
    /// configured with either the UDP or memory impl at startup.
    #[test]
    fn transport_trait_is_object_safe() {
        fn _accepts_dyn(_t: &dyn Transport<Error = TransportError>) {}
    }
}
