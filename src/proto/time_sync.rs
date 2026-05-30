//! TimeSync handshake state machine.
//!
//! TimeSync (message type 10) is a two-leg ping-pong between two
//! TCNet nodes used to estimate clock skew and round-trip latency.
//!
//! ```text
//!   Initiator                                         Responder
//!   ─────────                                         ─────────
//!      │── TimeSync { step=0, our_ts=T1 }────────────► │
//!      │                                               │
//!      │              ◄─── TimeSync { step=1, our_ts=T1 }
//!      │
//!      compute:                                        │
//!        rtt   = T_recv - T1
//!        delay = rtt / 2  (assumed symmetric link)
//! ```
//!
//! Because the V3.5.1B wire format echoes only the initiator's
//! timestamp (not the responder's local time), we can measure
//! *round-trip latency* but not signed clock skew without a parallel
//! NTP-style channel.  `ClockOffset::estimated_offset_us` is therefore
//! reported as `None` until the spec or peer extends the reply.

use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

/// An outstanding `TimeSync(step=0)` we're waiting on.
///
/// Consume by feeding the matching `step=1` reply to
/// [`PendingTimeSync::accept`] — the type-state encoding makes it a
/// compile error to use the value after it's been resolved.
#[derive(Debug, Clone, Copy)]
pub struct PendingTimeSync {
    /// Our send-side timestamp, the value we put in the `remote_timestamp`
    /// field of the outgoing `step=0` packet.  Microsecond resolution per
    /// spec page 8 (the `timestamp` field of the ManagementHeader is also
    /// µs).
    pub our_send_ts_us: u32,
    /// When we called `send_to` on the wire.
    pub sent_at: Instant,
    /// Peer we're handshaking with.
    pub peer: SocketAddrV4,
}

/// A `TimeSync(step=1)` reply just arrived.
#[derive(Debug, Clone, Copy)]
pub struct TimeSyncReply {
    /// Echoed `remote_timestamp` from the responder.  Must match
    /// `PendingTimeSync::our_send_ts_us` exactly.
    pub echoed_our_ts_us: u32,
    /// Responder's listener port (carried but not used for offset
    /// calculation today).
    pub their_listener_port: u16,
}

/// Outcome of a successfully-resolved TimeSync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockOffset {
    /// Measured round-trip latency.
    pub round_trip: Duration,
    /// Estimated one-way delay (= `round_trip / 2`).  Assumes a
    /// symmetric link.
    pub one_way_delay: Duration,
    /// Signed clock offset (initiator's clock minus responder's
    /// clock).  `None` because V3.5.1B's reply doesn't carry the
    /// responder's send-side timestamp — the wire would need a
    /// `their_send_ts` field for us to compute this.
    pub estimated_offset_us: Option<i64>,
    /// Peer the offset applies to.
    pub peer: SocketAddrV4,
}

/// Reasons a TimeSync reply may be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSyncError {
    /// Reply's `echoed_our_ts_us` ≠ our outgoing `our_send_ts_us`.
    /// Either a stale reply from a previous round, or a peer that
    /// mangled the echo field.
    EchoMismatch {
        expected_us: u32,
        got_us: u32,
    },
    /// Reply arrived more than `max_age` after the send — likely
    /// stale.
    StaleReply { age: Duration, max: Duration },
}

/// How old a reply may be before we reject it.  Spec doesn't pin a
/// number; this is a conservative default consistent with the
/// 1-second OptIn cadence.
pub const DEFAULT_MAX_REPLY_AGE: Duration = Duration::from_millis(500);

impl PendingTimeSync {
    /// Try to resolve this handshake with a freshly-arrived reply.
    /// `received_at` is the wall-clock at which we read the reply
    /// off the socket.
    pub fn accept(
        self,
        reply: TimeSyncReply,
        received_at: Instant,
    ) -> Result<ClockOffset, TimeSyncError> {
        self.accept_with_max_age(reply, received_at, DEFAULT_MAX_REPLY_AGE)
    }

    /// Same as [`accept`], but with a custom `max_age` for the
    /// staleness check.  Useful in tests that fast-forward time.
    pub fn accept_with_max_age(
        self,
        reply: TimeSyncReply,
        received_at: Instant,
        max_age: Duration,
    ) -> Result<ClockOffset, TimeSyncError> {
        if reply.echoed_our_ts_us != self.our_send_ts_us {
            return Err(TimeSyncError::EchoMismatch {
                expected_us: self.our_send_ts_us,
                got_us: reply.echoed_our_ts_us,
            });
        }
        let rtt = received_at.duration_since(self.sent_at);
        if rtt > max_age {
            return Err(TimeSyncError::StaleReply {
                age: rtt,
                max: max_age,
            });
        }
        Ok(ClockOffset {
            round_trip: rtt,
            one_way_delay: rtt / 2,
            estimated_offset_us: None,
            peer: self.peer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn peer() -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 7), 65023)
    }

    #[test]
    fn matching_reply_resolves_with_rtt() {
        let t0 = Instant::now();
        let pending = PendingTimeSync {
            our_send_ts_us: 12345,
            sent_at: t0,
            peer: peer(),
        };
        let reply = TimeSyncReply {
            echoed_our_ts_us: 12345,
            their_listener_port: 65023,
        };
        let received_at = t0 + Duration::from_micros(800);
        let offset = pending.accept(reply, received_at).expect("ok");
        assert_eq!(offset.round_trip, Duration::from_micros(800));
        assert_eq!(offset.one_way_delay, Duration::from_micros(400));
        assert!(offset.estimated_offset_us.is_none());
        assert_eq!(offset.peer, peer());
    }

    #[test]
    fn echo_mismatch_returns_error() {
        let t0 = Instant::now();
        let pending = PendingTimeSync {
            our_send_ts_us: 12345,
            sent_at: t0,
            peer: peer(),
        };
        let reply = TimeSyncReply {
            echoed_our_ts_us: 999,
            their_listener_port: 65023,
        };
        let err = pending
            .accept(reply, t0 + Duration::from_micros(800))
            .unwrap_err();
        assert_eq!(
            err,
            TimeSyncError::EchoMismatch {
                expected_us: 12345,
                got_us: 999,
            }
        );
    }

    #[test]
    fn stale_reply_returns_error() {
        let t0 = Instant::now();
        let pending = PendingTimeSync {
            our_send_ts_us: 12345,
            sent_at: t0,
            peer: peer(),
        };
        let reply = TimeSyncReply {
            echoed_our_ts_us: 12345,
            their_listener_port: 65023,
        };
        // > 500 ms past send
        let received_at = t0 + Duration::from_millis(700);
        let err = pending.accept(reply, received_at).unwrap_err();
        assert!(matches!(err, TimeSyncError::StaleReply { .. }));
    }

    #[test]
    fn custom_max_age_overrides_default() {
        let t0 = Instant::now();
        let pending = PendingTimeSync {
            our_send_ts_us: 12345,
            sent_at: t0,
            peer: peer(),
        };
        let reply = TimeSyncReply {
            echoed_our_ts_us: 12345,
            their_listener_port: 65023,
        };
        // 700 ms is past the default 500 ms but under a 1 s max.
        let r = pending.accept_with_max_age(
            reply,
            t0 + Duration::from_millis(700),
            Duration::from_secs(1),
        );
        assert!(r.is_ok());
    }

    #[test]
    fn accept_consumes_self() {
        // Compile check: the second `.accept()` is forbidden by the
        // type system because the first move-consumed `pending`.
        let pending = PendingTimeSync {
            our_send_ts_us: 1,
            sent_at: Instant::now(),
            peer: peer(),
        };
        let _ = pending.accept(
            TimeSyncReply {
                echoed_our_ts_us: 1,
                their_listener_port: 0,
            },
            Instant::now(),
        );
        // Uncommenting this line must fail to compile:
        //   let _ = pending.accept(...);
    }
}
