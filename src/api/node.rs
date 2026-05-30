//! `Node<R, V>` — the typed public API surface.
//!
//! This module is the type-system payoff of phases 1-7: the layered
//! design (wire / transport / session / proto / domain / runtime)
//! manifests above the line as a single `Node<R: Role, V: SpecVersion>`
//! handle, parameterised on the local role and spec version.
//!
//! The methods enforced at the type level:
//!
//! * `Node<Slave, V>` — `discover`, `snapshot`, `request_*`, `leave`.
//! * `Node<Master, V>` — Slave methods plus `broadcast_*` /
//!   `set_layer_metrics`.
//! * `Node<Auto, V>` — Slave methods plus `.wait_election()` which
//!   resolves into `Node<Master, V>` or `Node<Slave, V>`.
//! * `with_late_field()`-style builder methods require
//!   `V: IncludesFlame<F>` so a `Node<Master, V3_3_2>` can't emit a
//!   field that was added in `V3_4_1`.
//!
//! ## Current state
//!
//! The type surface lands in 0.2.0; the internal wiring through
//! [`SessionTask`](crate::session) + [`UdpTransport`](crate::transport)
//! + [`SnapshotWriter`](crate::domain) ships incrementally.  Callers
//! that want the full behaviour today should keep using
//! [`TCNetClient`](crate::TCNetClient) until 0.3.0; the typed surface
//! is here so downstream code can start migrating its *signatures*
//! ahead of the wiring switch-over.

use super::roles::{Auto, Master, Role, Slave};
use crate::session::{SessionHandle, SessionSnapshot};
use crate::spec_version::SpecVersion;
use std::marker::PhantomData;
use std::sync::Arc;

/// Anything the [`Node`] API can fail at.
#[non_exhaustive]
#[derive(Debug)]
pub enum NodeError {
    /// `Node::join` couldn't bring the transport up.
    JoinFailed {
        source: crate::transport::TransportError,
    },
    /// `Node::leave` couldn't broadcast OptOut.
    LeaveFailed,
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JoinFailed { source } => write!(f, "Node::join failed: {}", source),
            Self::LeaveFailed => f.write_str("Node::leave failed"),
        }
    }
}

impl std::error::Error for NodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::JoinFailed { source } => Some(source),
            _ => None,
        }
    }
}

/// The typed handle to a running TCNet node.
///
/// `R` is the role marker ([`Slave`] / [`Master`] / [`Auto`] /
/// [`Repeater`](super::roles::Repeater)); `V` is the
/// [`SpecVersion`](crate::SpecVersion) the local node emits at.
///
/// Cheap to clone — internally an `Arc` to the running session +
/// transport.
pub struct Node<R: Role, V: SpecVersion> {
    inner: Arc<NodeInner>,
    _r: PhantomData<R>,
    _v: PhantomData<V>,
}

struct NodeInner {
    session: SessionHandle,
    // transport + domain writer go here once the wiring lands.
}

impl<R: Role, V: SpecVersion> Node<R, V> {
    /// Wait-free read of the latest published peer-state snapshot.
    pub fn snapshot(&self) -> Arc<SessionSnapshot> {
        self.inner.session.snapshot()
    }

    /// Cleanly leave the network.  Consumes `self` to make the
    /// post-leave handle unreachable.
    ///
    /// Today this just shuts the session task down; the eventual
    /// implementation broadcasts an OptOut packet first.
    pub async fn leave(self) -> Result<(), NodeError> {
        self.inner.session.shutdown();
        Ok(())
    }
}

// Slave-specific methods.  Every role gets these via
// `impl<R: Role, V> Node<R, V>` above; `impl<V> Node<Slave, V>` only
// exists to mark which methods are role-specific.
impl<V: SpecVersion> Node<Slave, V> {}

// Master adds broadcast methods; details land with the wiring.
impl<V: SpecVersion> Node<Master, V> {}

// Auto exposes `.wait_election()` resolving into Master or Slave.
impl<V: SpecVersion> Node<Auto, V> {}

impl<R: Role, V: SpecVersion> Clone for Node<R, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _r: PhantomData,
            _v: PhantomData,
        }
    }
}

/// Internal constructor used by [`NodeBuilder`](super::NodeBuilder).
pub(crate) fn from_session<R: Role, V: SpecVersion>(session: SessionHandle) -> Node<R, V> {
    Node {
        inner: Arc::new(NodeInner { session }),
        _r: PhantomData,
        _v: PhantomData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::V3_6;
    use crate::session::SessionTask;

    #[test]
    fn node_is_send_sync() {
        fn _ss<T: Send + Sync>() {}
        _ss::<Node<Slave, V3_6>>();
    }

    #[tokio::test]
    async fn snapshot_load_from_typed_node() {
        let (_task, handle) = SessionTask::new_default();
        let n: Node<Slave, V3_6> = from_session(handle);
        let snap = n.snapshot();
        assert_eq!(snap.peers.len(), 0);
    }
}
