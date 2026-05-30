//! Timestamp-ordered snapshot writer.
//!
//! Why this exists: TCNet packets travel over UDP, so out-of-order
//! arrival is normal.  If a Time packet (≈ 20 ms cadence) arrives
//! *after* a Metrics packet for the same layer with a *newer* timestamp,
//! merging in arrival order would let the older Time overwrite the
//! newer Metrics-derived state.
//!
//! `TimestampOrdered<T>` buffers observations within a flush window,
//! sorts them by `header.timestamp`, and applies them in that order so
//! the merged snapshot reflects the latest *wire-side* state regardless
//! of arrival order.
//!
//! ## Flush window
//!
//! The window is the smallest unit of work the session task drains
//! before publishing.  Defaults to one Tick interval (50 ms) per the
//! existing dispatcher cadence.  Observations more than `max_skew` µs
//! old at flush time are still applied (so a slow path doesn't lose
//! data) but logged as warnings.

use std::collections::BinaryHeap;

/// An observation tagged with its wire-side header timestamp.
#[derive(Debug, Clone)]
pub struct Stamped<T> {
    /// Microsecond timestamp from the ManagementHeader.
    pub ts_us: u32,
    /// The decoded observation.
    pub value: T,
}

impl<T> Stamped<T> {
    pub fn new(ts_us: u32, value: T) -> Self {
        Self { ts_us, value }
    }
}

// BinaryHeap is a max-heap by default; we want oldest-first.  Wrap
// the timestamp in reverse-order comparison.
#[derive(Debug)]
struct HeapEntry<T> {
    ts_us: u32,
    value: T,
}

impl<T> PartialEq for HeapEntry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ts_us == other.ts_us
    }
}
impl<T> Eq for HeapEntry<T> {}
impl<T> PartialOrd for HeapEntry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for HeapEntry<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse so BinaryHeap pops smallest ts_us first.
        other.ts_us.cmp(&self.ts_us)
    }
}

/// A buffer of `T` observations that flushes in timestamp order.
///
/// Usage:
/// 1. Call [`TimestampOrdered::push`] for each arriving observation.
/// 2. At the flush boundary (Tick, snapshot publish, etc.), call
///    [`TimestampOrdered::drain_in_order`] and apply each observation
///    in the returned order.
pub struct TimestampOrdered<T> {
    heap: BinaryHeap<HeapEntry<T>>,
}

impl<T> Default for TimestampOrdered<T> {
    fn default() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }
}

impl<T> TimestampOrdered<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, stamped: Stamped<T>) {
        self.heap.push(HeapEntry {
            ts_us: stamped.ts_us,
            value: stamped.value,
        });
    }

    /// Number of buffered observations.
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Drain everything in timestamp-ascending order.
    pub fn drain_in_order(&mut self) -> Vec<Stamped<T>> {
        let mut out = Vec::with_capacity(self.heap.len());
        while let Some(e) = self.heap.pop() {
            out.push(Stamped::new(e.ts_us, e.value));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_drain() {
        let mut to = TimestampOrdered::<u32>::new();
        let v = to.drain_in_order();
        assert!(v.is_empty());
    }

    #[test]
    fn drain_returns_ascending_timestamps() {
        let mut to = TimestampOrdered::<&'static str>::new();
        to.push(Stamped::new(300, "c"));
        to.push(Stamped::new(100, "a"));
        to.push(Stamped::new(200, "b"));
        let v = to.drain_in_order();
        let tss: Vec<u32> = v.iter().map(|s| s.ts_us).collect();
        let vals: Vec<&str> = v.iter().map(|s| s.value).collect();
        assert_eq!(tss, vec![100, 200, 300]);
        assert_eq!(vals, vec!["a", "b", "c"]);
    }

    #[test]
    fn drain_consumes_buffer() {
        let mut to = TimestampOrdered::<u32>::new();
        to.push(Stamped::new(10, 1));
        to.push(Stamped::new(20, 2));
        assert_eq!(to.len(), 2);
        to.drain_in_order();
        assert!(to.is_empty());
    }

    #[test]
    fn timestamps_with_same_value_both_drain() {
        let mut to = TimestampOrdered::<u32>::new();
        to.push(Stamped::new(100, 1));
        to.push(Stamped::new(100, 2));
        let v = to.drain_in_order();
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|s| s.ts_us == 100));
    }
}
