//! Transport-layer error type.
//!
//! Anything the [`Transport`](super::Transport) trait can fail at lands
//! in [`TransportError`]. It's `#[non_exhaustive]` so we can grow it
//! over time without a SemVer bump (e.g. when phase 7 wires the RT
//! thread layer and adds `TickerOverrun`).

use std::io;
use std::net::SocketAddrV4;

/// Anything the transport layer can fail at.
#[non_exhaustive]
#[derive(Debug)]
pub enum TransportError {
    /// `UdpSocket::bind` failed for the given port.
    BindFailed {
        port: u16,
        source: io::Error,
    },
    /// `UdpSocket::send_to` failed for the given destination.
    SendFailed {
        dest: SocketAddrV4,
        source: io::Error,
    },
    /// The named channel's internal queue is closed (e.g. its owning
    /// task panicked).
    ChannelClosed { channel: super::channel::Channel },
    /// The buffer pool had no free slots — the recv loop fell behind.
    BufferPoolExhausted,
    /// `Transport::send` was given more bytes than the wire MTU
    /// permits. UDP datagrams over 65 507 bytes are spec-illegal; we
    /// reject at 8 192 to match the recv-side buffer pool.
    PayloadTooLarge { len: usize, max: usize },
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BindFailed { port, source } => {
                write!(f, "failed to bind UDP port {}: {}", port, source)
            }
            Self::SendFailed { dest, source } => {
                write!(f, "failed to send to {}: {}", dest, source)
            }
            Self::ChannelClosed { channel } => {
                write!(f, "channel {:?} closed", channel)
            }
            Self::BufferPoolExhausted => f.write_str("buffer pool exhausted"),
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload too large: {} bytes (max {})", len, max)
            }
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BindFailed { source, .. } => Some(source),
            Self::SendFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_error_is_send_sync_and_displays() {
        fn _is_send_sync<T: Send + Sync>() {}
        _is_send_sync::<TransportError>();

        let e = TransportError::PayloadTooLarge {
            len: 9000,
            max: 8192,
        };
        let s = format!("{e}");
        assert!(s.contains("9000"));
        assert!(s.contains("8192"));
    }
}
