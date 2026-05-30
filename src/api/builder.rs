//! `NodeBuilder` — fluent construction of typed `Node<R, V>` handles.
//!
//! Sensible defaults hide the Arc / atomic / transport choices the
//! library user shouldn't have to think about (matching the
//! "convenience stays inside the lib" constraint).  Override any
//! field via the builder fluent methods.

use super::node::{Node, NodeError, from_session};
use super::roles::Role;
use crate::ApplicationConfig;
use crate::session::SessionTask;
use crate::spec_version::SpecVersion;
use std::marker::PhantomData;
use std::net::Ipv4Addr;

/// Builder for [`Node`].
///
/// Construction is fluent: pick the role + spec version up front (as
/// type parameters), then chain `.with_*` methods, then `.spawn()`.
///
/// ```no_run
/// use tcnet::api::{NodeBuilder, roles::Slave};
/// use tcnet::V3_6;
///
/// # async fn _doc() -> Result<(), Box<dyn std::error::Error>> {
/// let node = NodeBuilder::<Slave, V3_6>::new()
///     .with_local_ip([127, 0, 0, 1].into())
///     .spawn()
///     .await?;
/// # Ok(()) }
/// ```
pub struct NodeBuilder<R: Role, V: SpecVersion> {
    config: ApplicationConfig,
    local_ip: Ipv4Addr,
    _r: PhantomData<R>,
    _v: PhantomData<V>,
}

impl<R: Role, V: SpecVersion> Default for NodeBuilder<R, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Role, V: SpecVersion> NodeBuilder<R, V> {
    /// Builder with library defaults: zero `node_id`, `0.0.0.0`
    /// local IP, "Default_" node name.
    pub fn new() -> Self {
        let mut config = ApplicationConfig::default();
        config.node_type = R::NODE_TYPE;
        Self {
            config,
            local_ip: Ipv4Addr::UNSPECIFIED,
            _r: PhantomData,
            _v: PhantomData,
        }
    }

    /// Override the local interface IP all four sockets bind to.
    pub fn with_local_ip(mut self, ip: Ipv4Addr) -> Self {
        self.local_ip = ip;
        self
    }

    /// Override the full [`ApplicationConfig`] in one go.  Any role
    /// mismatch is corrected — the builder always emits at `R::NODE_TYPE`.
    pub fn with_config(mut self, mut config: ApplicationConfig) -> Self {
        config.node_type = R::NODE_TYPE;
        self.config = config;
        self
    }

    /// Override the node id.
    pub fn with_node_id(mut self, id: u16) -> Self {
        self.config.node_id = id;
        self
    }

    /// Read back the configured local IP — useful for tests that
    /// want to assert the builder accepted their override.
    pub fn local_ip(&self) -> Ipv4Addr {
        self.local_ip
    }

    /// Read back the configured [`ApplicationConfig`].
    pub fn config(&self) -> &ApplicationConfig {
        &self.config
    }

    /// Spawn the node and return its typed handle.
    ///
    /// Today this only spawns the [`SessionTask`](crate::session); the
    /// transport + domain writer + RT-thread wiring lands in a
    /// follow-up commit on the same release line.  Until then,
    /// `spawn` is a no-network sentinel — the returned `Node` answers
    /// `snapshot()` from an empty session.
    pub async fn spawn(self) -> Result<Node<R, V>, NodeError> {
        let session = SessionTask::spawn();
        Ok(from_session(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::roles::{Master, Slave};
    use crate::V3_6;
    use crate::protocol::NodeType;

    #[test]
    fn builder_default_uses_role_node_type() {
        let b = NodeBuilder::<Slave, V3_6>::new();
        assert_eq!(b.config().node_type, NodeType::Slave);

        let b = NodeBuilder::<Master, V3_6>::new();
        assert_eq!(b.config().node_type, NodeType::Master);
    }

    #[test]
    fn with_config_corrects_role_mismatch() {
        let mut cfg = ApplicationConfig::default();
        cfg.node_type = NodeType::Master;
        let b = NodeBuilder::<Slave, V3_6>::new().with_config(cfg);
        assert_eq!(b.config().node_type, NodeType::Slave);
    }

    #[test]
    fn with_local_ip_records_value() {
        let b = NodeBuilder::<Slave, V3_6>::new().with_local_ip(Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(b.local_ip(), Ipv4Addr::new(10, 0, 0, 1));
    }

    #[tokio::test]
    async fn spawn_returns_typed_node() {
        let n = NodeBuilder::<Slave, V3_6>::new()
            .with_node_id(42)
            .spawn()
            .await
            .expect("spawn");
        let snap = n.snapshot();
        assert_eq!(snap.peers.len(), 0);
        n.leave().await.expect("leave");
    }
}
