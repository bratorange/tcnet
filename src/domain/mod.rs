//! Domain layer (ARCHITECTURE.md §6).
//!
//! Where the [`session`](crate::session) layer publishes peer-identity
//! facts, the domain layer publishes peer-*state* facts decoded from
//! DJ packets and merged across types.  Internal consistency
//! invariants (e.g. "stale Time packet must not clobber fresh
//! Metrics state") are enforced here via the timestamp-ordered
//! writer in [`writer`].
//!
//! ## Sub-modules
//!
//! * [`snapshot`] — `DomainLayerSnapshot` with `Option`-typed fields
//!   for the merge-across-packet-types fields.
//! * [`writer`] — `TimestampOrdered<T>` buffer that drains
//!   observations in `header.timestamp` order.

pub mod snapshot;
pub mod writer;

pub use snapshot::DomainLayerSnapshot;
pub use writer::{Stamped, TimestampOrdered};
