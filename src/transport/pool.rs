//! Lock-free buffer pool for inbound UDP datagrams.
//!
//! The recv side of [`Transport`](super::Transport) is per-packet
//! zero-alloc: the recv thread pulls an 8 KiB slot out of [`BufferPool`],
//! `recv_from`s into it, and hands the borrowed `&[u8]` up the stack.
//! When the parse is done, dropping the [`PoolBuf`] guard returns the
//! slot to the pool.
//!
//! The pool is a Treiber-stack-shaped [`ArrayQueue`] of `Box<[u8;
//! SLOT_SIZE]>`. Pop and push are wait-free, so the recv thread never
//! blocks on the pool.  If the pool is empty (every slot is checked
//! out by a slow parser), `acquire` returns `None` and the caller's
//! choices are documented per [`Transport`] impl — typically drop the
//! packet and bump a counter.
//!
//! ## Sizing
//!
//! `SLOT_SIZE = 8192` matches the existing `let mut buffer = [0;
//! 8192]` in `dispatcher::listen`.  TCNet packets fit comfortably:
//! Status / OptIn are ~120 bytes; the largest packets (BigWaveform /
//! ArtworkFile chunks) are ~1500 bytes.  We pick 8 KiB so a single
//! slot is always enough for one datagram.
//!
//! `capacity` is the number of slots.  64 is a sensible default —
//! enough for the recv thread + a handful of in-flight parses, well
//! short of memory bloat (64 × 8 KiB = 512 KiB).

use crossbeam_queue::ArrayQueue;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Per-slot byte capacity.  Matches the legacy `8192` constant in
/// `dispatcher::listen`.
pub const SLOT_SIZE: usize = 8192;

/// Lock-free pool of [`SLOT_SIZE`]-byte buffer slots.
///
/// Clone is `O(Arc-clone)` — all clones share the same underlying
/// freelist.
#[derive(Clone)]
pub struct BufferPool {
    inner: Arc<Inner>,
}

struct Inner {
    /// Freelist of slots ready to be checked out.  `ArrayQueue` is
    /// wait-free for both push and pop.
    free: ArrayQueue<Box<[u8; SLOT_SIZE]>>,
}

impl BufferPool {
    /// Allocate a new pool with `capacity` pre-allocated slots.
    ///
    /// All slots are allocated up-front so the recv thread doesn't pay
    /// for `Box::new` on a hot path.  `capacity == 0` is a degenerate
    /// pool that always returns `None` from `acquire`.
    pub fn new(capacity: usize) -> Self {
        let free = ArrayQueue::new(capacity.max(1));
        for _ in 0..capacity {
            // Allocate a zeroed slot.  Using `Box::new([0; N])` keeps
            // initialisation safe; the recv side will overwrite.
            let slot: Box<[u8; SLOT_SIZE]> = Box::new([0u8; SLOT_SIZE]);
            // Push can only fail if the queue is full — by construction
            // it isn't (we just created it with capacity slots).
            free.push(slot).ok();
        }
        Self {
            inner: Arc::new(Inner { free }),
        }
    }

    /// Check a slot out of the pool. Returns `None` if every slot is
    /// currently borrowed.
    ///
    /// The returned [`PoolBuf`] auto-returns the slot to the pool on
    /// `Drop`, so as long as the caller doesn't `mem::forget` it, no
    /// slot leaks.
    pub fn acquire(&self) -> Option<PoolBuf> {
        let buf = self.inner.free.pop()?;
        Some(PoolBuf {
            buf: Some(buf),
            pool: Arc::clone(&self.inner),
        })
    }

    /// Current number of slots checked back in (i.e. available).  For
    /// metrics / debugging.
    pub fn available(&self) -> usize {
        self.inner.free.len()
    }

    /// Total slots allocated by this pool (constant once constructed).
    pub fn capacity(&self) -> usize {
        self.inner.free.capacity()
    }
}

/// RAII guard for a checked-out [`BufferPool`] slot.
///
/// Deref / DerefMut as `[u8; SLOT_SIZE]`.  On drop, the slot is
/// returned to the pool.
pub struct PoolBuf {
    buf: Option<Box<[u8; SLOT_SIZE]>>,
    pool: Arc<Inner>,
}

impl Deref for PoolBuf {
    type Target = [u8; SLOT_SIZE];
    fn deref(&self) -> &Self::Target {
        // SAFETY: `buf` is only `None` inside `drop`; we never expose
        // a borrow after that point.
        self.buf.as_ref().expect("PoolBuf used after drop")
    }
}

impl DerefMut for PoolBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buf.as_mut().expect("PoolBuf used after drop")
    }
}

impl Drop for PoolBuf {
    fn drop(&mut self) {
        if let Some(buf) = self.buf.take() {
            // Push back into the freelist.  Can only fail if the pool
            // is somehow now full — shouldn't happen by construction,
            // but if it does we drop the slot rather than panic.
            let _ = self.pool.free.push(buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_starts_with_full_capacity() {
        let p = BufferPool::new(4);
        assert_eq!(p.capacity(), 4);
        assert_eq!(p.available(), 4);
    }

    #[test]
    fn acquire_consumes_a_slot_and_drop_returns_it() {
        let p = BufferPool::new(2);
        {
            let buf = p.acquire().expect("first acquire");
            assert_eq!(p.available(), 1);
            assert_eq!(buf.len(), SLOT_SIZE);
        }
        assert_eq!(p.available(), 2, "slot returned on drop");
    }

    #[test]
    fn acquire_returns_none_when_pool_exhausted() {
        let p = BufferPool::new(1);
        let _a = p.acquire().expect("first acquire");
        assert!(p.acquire().is_none(), "second acquire fails");
    }

    #[test]
    fn pool_is_clone_and_shares_state() {
        let p1 = BufferPool::new(2);
        let p2 = p1.clone();
        let _a = p1.acquire().expect("acquire via p1");
        assert_eq!(p2.available(), 1, "clone sees the depleted state");
    }

    #[test]
    fn pool_buf_is_writable_and_persists_across_releases() {
        let p = BufferPool::new(1);
        {
            let mut buf = p.acquire().expect("acquire");
            buf[0] = 0x42;
            buf[SLOT_SIZE - 1] = 0xAB;
        }
        let buf = p.acquire().expect("re-acquire same slot");
        // We don't guarantee zeroing on return — the recv side
        // overwrites before parsing.  Just assert we get *a* slot back.
        assert_eq!(buf.len(), SLOT_SIZE);
    }

    #[test]
    fn zero_capacity_is_a_degenerate_but_safe_pool() {
        let p = BufferPool::new(0);
        assert!(p.acquire().is_none());
    }

    #[test]
    fn pool_is_send_and_sync() {
        fn _ss<T: Send + Sync>() {}
        _ss::<BufferPool>();
    }
}
