//! Drift-corrected periodic ticker for the RT threads.
//!
//! TCNet's hot-path threads run at fixed cadences: 50 Hz session tick
//! (the Metrics broadcast / heartbeat interval), 1 Hz OptIn broadcast,
//! 50 Hz Time-packet emission.  A naive `thread::sleep(period)`
//! accumulates drift because each iteration's "real work" eats into
//! the next iteration's deadline.
//!
//! `Ticker` keeps a monotonic anchor (`Instant`) and computes the
//! *absolute* deadline for each tick — same idea as
//! `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, …)` on Linux but
//! portable across the cross-platform `std::thread::sleep_until`
//! approximation.
//!
//! Overruns are detected: if the wall-clock has already passed
//! `next_tick` by the time we look at it, the ticker emits a
//! `Lagged { by }` status and advances to the next future tick.  The
//! callers' workload — `SessionTask::run` is the canonical one —
//! uses the lagged count to back off + log when the host CPU can't
//! keep up.

use std::thread;
use std::time::{Duration, Instant};

/// One iteration's worth of timing status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickStatus {
    /// We slept until the deadline and woke exactly on time.
    OnTime,
    /// We woke late — caller's previous iteration took longer than
    /// `period`.  `by` is how far behind we were before the ticker
    /// caught up.
    Lagged { by: Duration },
}

/// A drift-corrected periodic ticker.
///
/// Construct with [`Ticker::new`].  Each loop iteration calls
/// [`Ticker::wait`] to block until the next deadline; the returned
/// [`TickStatus`] reports whether we made it on time.
pub struct Ticker {
    period: Duration,
    next_deadline: Instant,
    /// Cumulative overrun count for diagnostics.
    overruns: u64,
}

impl Ticker {
    /// Build a ticker that fires every `period`, with the first tick
    /// landing at `Instant::now() + period`.
    pub fn new(period: Duration) -> Self {
        let now = Instant::now();
        Self {
            period,
            next_deadline: now + period,
            overruns: 0,
        }
    }

    /// Build a ticker whose first deadline is `start_at`.  Useful for
    /// phase-aligned tickers (e.g. one starts at `t+0`, the next at
    /// `t+10 ms`).
    pub fn new_at(start_at: Instant, period: Duration) -> Self {
        Self {
            period,
            next_deadline: start_at,
            overruns: 0,
        }
    }

    /// Block the calling thread until the next tick.  Returns
    /// [`TickStatus::OnTime`] if we slept the full remaining
    /// interval, [`TickStatus::Lagged`] if we were already past the
    /// deadline at entry.
    ///
    /// `wait` is monotone with respect to overruns: each call advances
    /// `next_deadline` by at least one `period`, even if several
    /// periods elapsed since the previous call.
    pub fn wait(&mut self) -> TickStatus {
        let now = Instant::now();
        if now < self.next_deadline {
            let sleep_for = self.next_deadline - now;
            thread::sleep(sleep_for);
            self.next_deadline += self.period;
            TickStatus::OnTime
        } else {
            let by = now - self.next_deadline;
            self.overruns = self.overruns.saturating_add(1);
            // Advance to the next future deadline so we don't fire
            // burst-mode trying to catch up.
            let periods_missed = (by.as_nanos() / self.period.as_nanos().max(1)).max(1) as u32;
            self.next_deadline += self.period * periods_missed;
            // If we're still in the past, advance one more period.
            while self.next_deadline <= now {
                self.next_deadline += self.period;
            }
            TickStatus::Lagged { by }
        }
    }

    /// Period this ticker is configured for.
    pub fn period(&self) -> Duration {
        self.period
    }

    /// Cumulative count of `Lagged` returns over the ticker's lifetime.
    pub fn overruns(&self) -> u64 {
        self.overruns
    }

    /// Instant of the next deadline.  Useful for instrumentation.
    pub fn next_deadline(&self) -> Instant {
        self.next_deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_wait_blocks_for_approximately_period() {
        let mut t = Ticker::new(Duration::from_millis(20));
        let start = Instant::now();
        let status = t.wait();
        let elapsed = start.elapsed();
        assert!(matches!(status, TickStatus::OnTime));
        // Allow generous tolerance for CI noise.
        assert!(
            elapsed >= Duration::from_millis(15),
            "elapsed = {elapsed:?}"
        );
        assert!(
            elapsed <= Duration::from_millis(150),
            "elapsed = {elapsed:?}"
        );
    }

    #[test]
    fn lagged_status_reported_when_caller_overruns() {
        let mut t = Ticker::new(Duration::from_millis(10));
        // Simulate a heavy iteration by sleeping past the deadline.
        thread::sleep(Duration::from_millis(40));
        let status = t.wait();
        match status {
            TickStatus::Lagged { by } => {
                assert!(by >= Duration::from_millis(20), "by = {by:?}");
            }
            other => panic!("expected Lagged, got {other:?}"),
        }
        assert_eq!(t.overruns(), 1);
    }

    #[test]
    fn lagged_advances_next_deadline_past_now() {
        let mut t = Ticker::new(Duration::from_millis(10));
        thread::sleep(Duration::from_millis(40));
        let _ = t.wait();
        // Next deadline should be strictly in the future.
        assert!(t.next_deadline() > Instant::now());
    }

    #[test]
    fn period_accessor_returns_configured_value() {
        let t = Ticker::new(Duration::from_millis(25));
        assert_eq!(t.period(), Duration::from_millis(25));
    }
}
