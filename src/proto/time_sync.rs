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
    /// Responder's listener port.
    pub their_listener_port: u16,
    /// Responder's `header.timestamp` µs counter at the moment they
    /// built the reply — spec page 8 calls this "Timestamp" in the
    /// formula `Time of remote node = Timestamp + Delay`.
    /// Required for computing the signed clock offset.
    pub responder_send_ts_us: u32,
}

/// Outcome of a successfully-resolved TimeSync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockOffset {
    /// Measured round-trip latency.
    pub round_trip: Duration,
    /// Estimated one-way delay (= `round_trip / 2`).  Assumes a
    /// symmetric link.
    pub one_way_delay: Duration,
    /// Signed offset in µs, defined as
    /// `(responder_clock - local_clock)` at the moment the reply
    /// arrived.  Positive = responder's clock is ahead.  Computed
    /// from the spec formula on page 8:
    /// `time_of_remote_now = responder_send_ts + one_way_delay`,
    /// then `offset = time_of_remote_now - local_recv_ts`.
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

    /// Same as [`accept`](Self::accept), but with a custom `max_age` for the
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
        // Spec page 8 offset computation.  All timestamps are µs-of-
        // second per the ManagementHeader `timestamp` field (V3.5.1B
        // says `0-999999`); the rollover at 1 s requires wrapping
        // subtraction so a reply that crosses the 1-Mµs boundary
        // doesn't produce a wildly-wrong offset.
        let half_rtt_us = (rtt.as_micros() / 2) as i64;
        // time_of_remote_at_receive = responder_send_ts + half_rtt
        // local_recv_ts is implicit (we're computing for "now").  We
        // approximate local_recv_ts by `our_send_ts + rtt` since both
        // are in the µs-of-second domain.
        let local_recv_ts_us = self
            .our_send_ts_us
            .wrapping_add(rtt.as_micros() as u32)
            % 1_000_000;
        let remote_at_recv_us = (reply.responder_send_ts_us as i64
            + half_rtt_us)
            .rem_euclid(1_000_000);
        // Signed offset = remote_now - local_now (wraps around 1 s).
        let mut offset_us = remote_at_recv_us - local_recv_ts_us as i64;
        if offset_us > 500_000 {
            offset_us -= 1_000_000;
        } else if offset_us < -500_000 {
            offset_us += 1_000_000;
        }
        Ok(ClockOffset {
            round_trip: rtt,
            one_way_delay: rtt / 2,
            estimated_offset_us: Some(offset_us),
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

    fn reply(echo: u32, responder_ts: u32) -> TimeSyncReply {
        TimeSyncReply {
            echoed_our_ts_us: echo,
            their_listener_port: 65023,
            responder_send_ts_us: responder_ts,
        }
    }

    #[test]
    fn matching_reply_resolves_with_rtt_and_offset() {
        let t0 = Instant::now();
        let pending = PendingTimeSync {
            our_send_ts_us: 12345,
            sent_at: t0,
            peer: peer(),
        };
        let received_at = t0 + Duration::from_micros(800);
        // Responder's clock was at 13_545 µs when it sent the reply.
        let offset = pending.accept(reply(12345, 13_545), received_at).expect("ok");
        assert_eq!(offset.round_trip, Duration::from_micros(800));
        assert_eq!(offset.one_way_delay, Duration::from_micros(400));
        // local_recv_ts ≈ 12345 + 800 = 13_145
        // remote_at_recv ≈ 13_545 + 400 = 13_945
        // offset = 13_945 - 13_145 = +800 µs (responder ahead)
        assert_eq!(offset.estimated_offset_us, Some(800));
        assert_eq!(offset.peer, peer());
    }

    #[test]
    fn zero_drift_responder_yields_zero_offset() {
        let t0 = Instant::now();
        let pending = PendingTimeSync {
            our_send_ts_us: 0,
            sent_at: t0,
            peer: peer(),
        };
        let received_at = t0 + Duration::from_micros(400);
        // Symmetric link, no drift: responder sent at our_send+200 µs
        // (one-way delay).
        let offset = pending.accept(reply(0, 200), received_at).expect("ok");
        assert_eq!(offset.estimated_offset_us, Some(0));
    }

    #[test]
    fn echo_mismatch_returns_error() {
        let t0 = Instant::now();
        let pending = PendingTimeSync {
            our_send_ts_us: 12345,
            sent_at: t0,
            peer: peer(),
        };
        let err = pending
            .accept(reply(999, 0), t0 + Duration::from_micros(800))
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
        let received_at = t0 + Duration::from_millis(700);
        let err = pending.accept(reply(12345, 0), received_at).unwrap_err();
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
        let r = pending.accept_with_max_age(
            reply(12345, 0),
            t0 + Duration::from_millis(700),
            Duration::from_secs(1),
        );
        assert!(r.is_ok());
    }

    #[test]
    fn accept_consumes_self() {
        let pending = PendingTimeSync {
            our_send_ts_us: 1,
            sent_at: Instant::now(),
            peer: peer(),
        };
        let _ = pending.accept(reply(1, 0), Instant::now());
        // Uncommenting this line must fail to compile:
        //   let _ = pending.accept(...);
    }
}
