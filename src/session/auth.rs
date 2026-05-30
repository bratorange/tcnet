//! Peer authentication state.
//!
//! The TCNet spec mentions an authentication handshake but doesn't
//! describe the wire format in V3.5.1B.  This module is a *scaffold*:
//! the [`PeerAuth`] enum names the three states a peer can be in, but
//! no actual handshake runs — every peer stays `Anonymous` for now.
//!
//! Phase 5 still uses [`PeerAuth::Authenticated`] as a gating witness
//! for outbound Control / Text — the privileged builder methods take
//! `&PeerActive<Authenticated>` so they're unreachable unless the
//! session task has elevated the peer.  When the handshake spec
//! materialises, the wiring under that bound is the only thing that
//! needs to change.

/// Runtime authentication state of a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerAuth {
    /// We've never asked the peer for credentials.  Every peer starts
    /// here, and today every peer stays here.
    Anonymous,
    /// The peer's [`NodeOptions`](crate::protocol::NodeOptions) bit
    /// for authentication is set, but no handshake has completed.
    /// Privileged outbound messages must wait.
    AuthRequired,
    /// Handshake complete.  Privileged outbound messages are
    /// authorised.
    Authenticated,
}

impl Default for PeerAuth {
    fn default() -> Self {
        Self::Anonymous
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_auth_default_is_anonymous() {
        assert_eq!(PeerAuth::default(), PeerAuth::Anonymous);
    }

    #[test]
    fn peer_auth_variants_are_distinguishable() {
        assert_ne!(PeerAuth::Anonymous, PeerAuth::AuthRequired);
        assert_ne!(PeerAuth::AuthRequired, PeerAuth::Authenticated);
        assert_ne!(PeerAuth::Anonymous, PeerAuth::Authenticated);
    }
}
