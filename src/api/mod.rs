//! Typed public API.
//!
//! [`Node<R, V>`](Node) is the single handle every consumer interacts
//! with.  Construct it via [`NodeBuilder`]; read state via
//! [`Node::snapshot`]; tear it down via `node.leave().await`.
//!
//! See the [`node`] module for the typed-method layout.
//!
//! ## Sub-modules
//!
//! * [`roles`] — sealed `Role` trait + `Slave` / `Master` / `Auto` /
//!   `Repeater` marker types.  Role determines which methods are
//!   reachable: `broadcast_*` / `set_*` are gated on `R = Master`.
//! * [`node`] — `Node<R, V>`, `NodeError`, `NodeSnapshot`, `PeerInfo`.
//! * [`builder`] — fluent `NodeBuilder` construction.

pub mod builder;
pub mod node;
pub mod roles;

pub use builder::NodeBuilder;
pub use node::{Node, NodeError, NodeSnapshot, PeerInfo};
pub use roles::{Auto, Master, Repeater, Role, Slave};
