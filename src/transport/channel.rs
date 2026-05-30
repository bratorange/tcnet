//! Transport channel taxonomy.
//!
//! The TCNet wire model is four UDP sockets, each with its own purpose
//! and timing characteristics (spec page 7). The [`Channel`] enum names
//! them; [`ChannelConfig`] / [`OverflowPolicy`] / [`ChannelStatus`]
//! parameterise and report on per-channel queue health.
//!
//! This module is deliberately data-only — no socket logic lives here.
//! The actual transport impls in [`udp`](super::udp) / [`memory`] hold
//! the sockets and the queues; they consult `ChannelConfig` to decide
//! what to do when a queue is full.

/// One of the four TCNet wire channels.
///
/// Each port is fixed by the spec (table page 7):
///
/// | Channel              | Port  | Spec role                    |
/// |---------------------|-------|------------------------------|
/// | `Broadcast60000`    | 60000 | Status / OptIn / Control     |
/// | `Time60001`         | 60001 | 20-ms time packets           |
/// | `Broadcast60002`    | 60002 | Reserved broadcast           |
/// | `Unicast`           | 65023 | Per-peer unicast (req/resp)  |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Broadcast60000,
    Time60001,
    Broadcast60002,
    Unicast,
}

impl Channel {
    /// Static port number from the spec table.
    pub const fn port(self) -> u16 {
        match self {
            Self::Broadcast60000 => 60000,
            Self::Time60001 => 60001,
            Self::Broadcast60002 => 60002,
            Self::Unicast => 65023,
        }
    }

    /// All four channels in spec table order.
    pub const fn all() -> [Self; 4] {
        [
            Self::Broadcast60000,
            Self::Time60001,
            Self::Broadcast60002,
            Self::Unicast,
        ]
    }
}

/// What the transport does when a channel's send queue fills up.
///
/// Each channel picks its own policy at startup; the choice is a
/// trade-off between latency (drop newest = keep old data flowing) and
/// freshness (drop oldest = keep new data flowing). Status / Time
/// channels prefer `DropOldest` (stale time packets are worthless);
/// Unicast request/response prefers `DropNewest` so the in-flight
/// request can finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Pop the head of the queue, push the new item to the tail.
    /// Caller never blocks; lost-data counter increments.
    DropOldest,
    /// Drop the new item on the floor. Caller never blocks; if
    /// `warn` is true a `log::warn!` fires (rate-limited by the impl).
    DropNewest { warn: bool },
    /// Block the caller until space is available.  Only safe on
    /// cold-path (user-facing async) callers; never select this for
    /// the recv → session hand-off.
    BackPressureAsync,
}

/// Per-channel queue sizing + overflow handling.
#[derive(Debug, Clone, Copy)]
pub struct ChannelConfig {
    /// Maximum number of in-flight datagrams the channel's queue may
    /// hold.  `8` is a sensible default for control channels; `64` for
    /// the Time channel where bursts are normal.
    pub capacity: usize,
    /// What to do when the queue is full.
    pub overflow: OverflowPolicy,
}

impl ChannelConfig {
    /// Defaults that match the spec's expected cadences:
    ///
    /// * Broadcast60000 — `capacity 16, DropOldest`
    /// * Time60001 — `capacity 64, DropOldest`
    /// * Broadcast60002 — `capacity 8, DropOldest`
    /// * Unicast — `capacity 32, DropNewest { warn: true }`
    pub const fn default_for(channel: Channel) -> Self {
        match channel {
            Channel::Broadcast60000 => Self {
                capacity: 16,
                overflow: OverflowPolicy::DropOldest,
            },
            Channel::Time60001 => Self {
                capacity: 64,
                overflow: OverflowPolicy::DropOldest,
            },
            Channel::Broadcast60002 => Self {
                capacity: 8,
                overflow: OverflowPolicy::DropOldest,
            },
            Channel::Unicast => Self {
                capacity: 32,
                overflow: OverflowPolicy::DropNewest { warn: true },
            },
        }
    }
}

/// Read-only snapshot of a channel's queue health.
///
/// Returned by [`Transport::channel_status`](super::Transport::channel_status).
/// Cheap to copy; transports publish it via [`arc_swap::ArcSwap`] so
/// reads don't lock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChannelStatus {
    /// Current queue depth (items waiting to be sent or consumed).
    pub queue_len: usize,
    /// Configured capacity (snapshot of `ChannelConfig.capacity`).
    pub queue_cap: usize,
    /// Cumulative number of datagrams dropped by overflow policy.
    pub dropped: u64,
    /// Cumulative number of datagrams successfully sent / received.
    pub processed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_port_matches_spec_table() {
        assert_eq!(Channel::Broadcast60000.port(), 60000);
        assert_eq!(Channel::Time60001.port(), 60001);
        assert_eq!(Channel::Broadcast60002.port(), 60002);
        assert_eq!(Channel::Unicast.port(), 65023);
    }

    #[test]
    fn channel_all_returns_all_four_in_order() {
        assert_eq!(
            Channel::all(),
            [
                Channel::Broadcast60000,
                Channel::Time60001,
                Channel::Broadcast60002,
                Channel::Unicast,
            ]
        );
    }

    #[test]
    fn default_config_picks_drop_oldest_for_broadcast_channels() {
        for ch in [
            Channel::Broadcast60000,
            Channel::Time60001,
            Channel::Broadcast60002,
        ] {
            assert_eq!(
                ChannelConfig::default_for(ch).overflow,
                OverflowPolicy::DropOldest
            );
        }
    }

    #[test]
    fn default_config_picks_drop_newest_warn_for_unicast() {
        let cfg = ChannelConfig::default_for(Channel::Unicast);
        assert_eq!(cfg.overflow, OverflowPolicy::DropNewest { warn: true });
        assert_eq!(cfg.capacity, 32);
    }
}
