//! Role markers for `Node<R, V>`.
//!
//! The TCNet spec lets a node declare itself as Slave, Master, Auto
//! (let-the-network-decide), or Repeater.  Each marker carries the
//! [`NodeType`](crate::protocol::NodeType) the local node announces on
//! the wire, and gates the public method surface:
//!
//! * `Slave` — announces `NodeType::Slave`. Read-only: discovers peers,
//!   snapshots their state, requests waveform/beat-grid/etc.
//! * `Master` — announces `NodeType::Master`. Read surface plus the
//!   broadcaster handle (`Deref` to [`ActiveDJNode`](crate::ActiveDJNode)):
//!   Status / Time / Metrics / Meta / Mixer emission and set-layer-metrics.
//! * `Auto` — announces `NodeType::Auto`, so it is an election candidate
//!   (see [`Node::election_state`](crate::api::Node::election_state)).
//!   Same read-only surface as `Slave`; carries no broadcaster.
//! * `Repeater` — announces `NodeType::Repeater` (TCNet bridge,
//!   untyped in V3.5.1B). Read-only surface.
//!
//! All four expose the same read methods; only `Master` adds the write
//! surface. Sealed trait so consumers can't extend the role set.

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
