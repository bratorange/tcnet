//! Role markers for `Node<R, V>`.
//!
//! The TCNet spec lets a node declare itself as Slave, Master, Auto
//! (let-the-network-decide), or Repeater.  The role determines what
//! methods make sense on the public surface:
//!
//! * `Slave` — receive-only.  May discover peers, snapshot their
//!   state, request waveform/beat-grid/etc., but doesn't broadcast.
//! * `Master` — Slave methods plus broadcast Status / Time / Metrics
//!   / Meta / Mixer and set-layer-metrics.
//! * `Auto` — Slave methods plus a `.wait_election()` future that
//!   resolves into `Node<Master>` or `Node<Slave>` depending on
//!   election outcome.
//! * `Repeater` — TCNet bridge (untyped in V3.5.1B; treated as
//!   Slave-equivalent here, surface-level).
//!
//! Sealed trait so consumers can't extend the role set.

mod sealed {
    pub trait RoleSealed {}
}

/// One of the four spec roles.
pub trait Role: sealed::RoleSealed + Copy + Default + 'static {
    /// Wire-level `NodeType` value carried in the ManagementHeader.
    const NODE_TYPE: crate::protocol::NodeType;
}

macro_rules! decl_role {
    ($name:ident, $node_type:expr) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name;
        impl sealed::RoleSealed for $name {}
        impl Role for $name {
            const NODE_TYPE: crate::protocol::NodeType = $node_type;
        }
    };
}

decl_role!(Slave, crate::protocol::NodeType::Slave);
decl_role!(Master, crate::protocol::NodeType::Master);
decl_role!(Auto, crate::protocol::NodeType::Auto);
decl_role!(Repeater, crate::protocol::NodeType::Repeater);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::NodeType;

    #[test]
    fn role_node_type_matches_marker() {
        assert_eq!(Slave::NODE_TYPE, NodeType::Slave);
        assert_eq!(Master::NODE_TYPE, NodeType::Master);
        assert_eq!(Auto::NODE_TYPE, NodeType::Auto);
        assert_eq!(Repeater::NODE_TYPE, NodeType::Repeater);
    }
}
