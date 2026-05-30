//! Typed public API (ARCHITECTURE.md §8).
//!
//! `Node<R, V>` is the single handle every consumer interacts with.
//! Construct it via [`NodeBuilder`]; access its state via
//! `node.snapshot()`; tear it down via `node.leave().await`.
//!
//! See the module docs of [`node`] for the typed-method layout.
//!
//! Migration note (0.1 → 0.2):
//! - `TCNetClient` / `ActiveDJNode` / `DjControllerView` stay in the
//!   crate root in 0.2.0 — the typed surface ships in parallel and
//!   downstream code can migrate at its own pace.
//! - The legacy surface is deprecated for 0.3.0 removal.
//!
//! ## Sub-modules
//!
//! * [`roles`] — sealed `Role` trait + `Slave` / `Master` / `Auto`
//!   / `Repeater` marker types.
//! * [`node`] — `Node<R, V>` + `NodeError`.
//! * [`builder`] — `NodeBuilder` fluent construction.

pub mod builder;
pub mod node;
pub mod roles;

pub use builder::NodeBuilder;
pub use node::{Node, NodeError, NodeSnapshot, PeerInfo};
pub use roles::{Auto, Master, Repeater, Role, Slave};
