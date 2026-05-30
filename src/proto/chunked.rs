//! Generic chunk reassembly for multi-packet TCNet responses.
//!
//! TCNet replies that don't fit in one UDP datagram (Beat Grid, Big
//! Waveform, Artwork File, AppSpecific 30+213, …) come as a sequence
//! of chunks tagged with `(packet_no, total_packets, data_size)`.
//! The receiver buffers each chunk by `packet_no`, and once every
//! chunk has arrived flattens them into the assembled payload.
//!
//! The legacy `src/node/dj_controller.rs` has three identical
//! accumulator structs hand-coded for Beat Grid / Big Waveform /
//! Artwork.  This generic replaces all three (and is the home for
//! the AppSpecific accumulator in 5.5).
//!
//! ## Example
//!
//! ```text
//! use tcnet::proto::{ChunkedFrame, ChunkedPayload, AcceptOutcome};
//!
//! struct MyChunk { packet_no: u32, total_packets: u32, data_size: u32,
//!                  layer_id: u8, bytes: Vec<u8> }
//! impl ChunkedPayload for MyChunk {
//!     type Assembled = Vec<u8>;
//!     fn packet_no(&self) -> u32 { self.packet_no }
//!     fn total_packets(&self) -> u32 { self.total_packets }
//!     fn data_size(&self) -> u32 { self.data_size }
//!     fn layer_id(&self) -> u8 { self.layer_id }
//!     fn chunk_bytes(&self) -> &[u8] { &self.bytes }
//!     fn assemble(_layer_id: u8, bytes: Vec<u8>) -> Self::Assembled { bytes }
//! }
//! ```

use std::marker::PhantomData;

/// A single chunk of a multi-packet TCNet response.
///
/// Implementors expose the four wire-derived counters and the
/// payload slice; `assemble` finalises the buffered bytes into the
/// caller's chosen domain type.
pub trait ChunkedPayload {
    /// What the accumulated chunks reassemble into.
    type Assembled;

    /// Sequence number within this response (0-indexed).
    fn packet_no(&self) -> u32;
    /// Total expected packets — same value on every chunk of a given
    /// response.  Sanity-checked across chunks.
    fn total_packets(&self) -> u32;
    /// Total payload size in bytes from the first chunk.
    fn data_size(&self) -> u32;
    /// Layer identifier carried by the response.
    fn layer_id(&self) -> u8;
    /// This chunk's payload bytes.
    fn chunk_bytes(&self) -> &[u8];

    /// Stitch the flat byte buffer into the assembled output type.
    fn assemble(layer_id: u8, bytes: Vec<u8>) -> Self::Assembled;
}

/// Outcome of feeding one chunk to [`ChunkedFrame::accept`].
#[derive(Debug)]
pub enum AcceptOutcome<A> {
    /// More chunks needed; assembly not yet complete.
    NeedMore,
    /// All chunks received and flattened.
    Complete(A),
    /// Same `packet_no` arrived twice — second instance dropped.
    Duplicate,
    /// Chunk metadata disagreed with what previous chunks declared.
    /// Most commonly a peer changed `total_packets` mid-flight.
    Mismatch { reason: MismatchReason },
}

/// Why a chunk was rejected as mismatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchReason {
    /// Chunk's `total_packets` ≠ accumulator's expected total.
    TotalPacketsChanged,
    /// Chunk's `layer_id` ≠ accumulator's expected layer.
    LayerChanged,
    /// Chunk's `packet_no >= total_packets` — out-of-range.
    PacketNoOutOfRange,
}

/// Reassembler for a single multi-packet response.
///
/// Construct fresh per response (do not reuse across responses).  Feed
/// each arriving chunk to [`ChunkedFrame::accept`] until it returns
/// [`AcceptOutcome::Complete`].
pub struct ChunkedFrame<T: ChunkedPayload> {
    chunks: Vec<Option<Vec<u8>>>,
    layer_id: Option<u8>,
    total_packets: u32,
    received_count: u32,
    total_data_size: u32,
    _phantom: PhantomData<T>,
}

impl<T: ChunkedPayload> Default for ChunkedFrame<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ChunkedPayload> ChunkedFrame<T> {
    /// Empty accumulator.  Sized lazily on the first chunk.
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            layer_id: None,
            total_packets: 0,
            received_count: 0,
            total_data_size: 0,
            _phantom: PhantomData,
        }
    }

    /// Number of unique chunks accumulated so far.
    pub fn received_count(&self) -> u32 {
        self.received_count
    }

    /// Total packets expected (zero until the first chunk arrives).
    pub fn total_packets(&self) -> u32 {
        self.total_packets
    }

    /// Has any chunk been received?
    pub fn is_empty(&self) -> bool {
        self.received_count == 0
    }

    /// Feed one chunk.  See [`AcceptOutcome`] for the possible
    /// outcomes.
    pub fn accept(&mut self, packet: T) -> AcceptOutcome<T::Assembled> {
        let pkt_no = packet.packet_no();
        let total = packet.total_packets().max(1);
        let layer = packet.layer_id();

        // First chunk — size the buffer.
        if self.layer_id.is_none() {
            self.layer_id = Some(layer);
            self.total_packets = total;
            self.total_data_size = packet.data_size();
            self.chunks.resize(total as usize, None);
        } else {
            // Validate against expectations from the first chunk.
            if self.total_packets != total {
                return AcceptOutcome::Mismatch {
                    reason: MismatchReason::TotalPacketsChanged,
                };
            }
            if self.layer_id != Some(layer) {
                return AcceptOutcome::Mismatch {
                    reason: MismatchReason::LayerChanged,
                };
            }
        }

        if pkt_no >= total {
            return AcceptOutcome::Mismatch {
                reason: MismatchReason::PacketNoOutOfRange,
            };
        }

        let slot = &mut self.chunks[pkt_no as usize];
        if slot.is_some() {
            return AcceptOutcome::Duplicate;
        }
        *slot = Some(packet.chunk_bytes().to_vec());
        self.received_count += 1;

        if self.received_count < self.total_packets {
            return AcceptOutcome::NeedMore;
        }

        // Complete — flatten in order.
        let mut buf = Vec::with_capacity(self.total_data_size as usize);
        for slot in &self.chunks {
            // SAFETY: received_count == total_packets means every
            // slot is Some.  The `.unwrap_or` keeps us panic-free if
            // a stale invariant ever slips.
            if let Some(b) = slot {
                buf.extend_from_slice(b);
            }
        }
        let assembled = T::assemble(self.layer_id.unwrap_or(0), buf);
        AcceptOutcome::Complete(assembled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct Chunk {
        packet_no: u32,
        total_packets: u32,
        data_size: u32,
        layer_id: u8,
        bytes: Vec<u8>,
    }

    impl ChunkedPayload for Chunk {
        type Assembled = (u8, Vec<u8>);
        fn packet_no(&self) -> u32 {
            self.packet_no
        }
        fn total_packets(&self) -> u32 {
            self.total_packets
        }
        fn data_size(&self) -> u32 {
            self.data_size
        }
        fn layer_id(&self) -> u8 {
            self.layer_id
        }
        fn chunk_bytes(&self) -> &[u8] {
            &self.bytes
        }
        fn assemble(layer_id: u8, bytes: Vec<u8>) -> Self::Assembled {
            (layer_id, bytes)
        }
    }

    fn c(packet_no: u32, total: u32, layer: u8, bytes: &[u8]) -> Chunk {
        Chunk {
            packet_no,
            total_packets: total,
            data_size: (bytes.len() * total as usize) as u32,
            layer_id: layer,
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn single_chunk_completes_immediately() {
        let mut f = ChunkedFrame::<Chunk>::new();
        let result = f.accept(c(0, 1, 3, b"abcd"));
        match result {
            AcceptOutcome::Complete((layer, bytes)) => {
                assert_eq!(layer, 3);
                assert_eq!(bytes, b"abcd");
            }
            other => panic!("expected Complete, got {other:?}"),
        }
    }

    #[test]
    fn multi_chunk_needs_all_before_completing() {
        let mut f = ChunkedFrame::<Chunk>::new();
        assert!(matches!(f.accept(c(0, 3, 1, b"AA")), AcceptOutcome::NeedMore));
        assert!(matches!(f.accept(c(1, 3, 1, b"BB")), AcceptOutcome::NeedMore));
        let r = f.accept(c(2, 3, 1, b"CC"));
        match r {
            AcceptOutcome::Complete((layer, bytes)) => {
                assert_eq!(layer, 1);
                assert_eq!(bytes, b"AABBCC");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn multi_chunk_out_of_order_still_completes() {
        let mut f = ChunkedFrame::<Chunk>::new();
        assert!(matches!(f.accept(c(2, 3, 1, b"CC")), AcceptOutcome::NeedMore));
        assert!(matches!(f.accept(c(0, 3, 1, b"AA")), AcceptOutcome::NeedMore));
        let r = f.accept(c(1, 3, 1, b"BB"));
        match r {
            AcceptOutcome::Complete((_, bytes)) => assert_eq!(bytes, b"AABBCC"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn duplicate_chunk_is_ignored() {
        let mut f = ChunkedFrame::<Chunk>::new();
        f.accept(c(0, 2, 1, b"AA"));
        assert!(matches!(f.accept(c(0, 2, 1, b"AA")), AcceptOutcome::Duplicate));
        assert_eq!(f.received_count(), 1);
        let r = f.accept(c(1, 2, 1, b"BB"));
        match r {
            AcceptOutcome::Complete((_, bytes)) => assert_eq!(bytes, b"AABB"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn total_packets_change_is_a_mismatch() {
        let mut f = ChunkedFrame::<Chunk>::new();
        f.accept(c(0, 3, 1, b"AA"));
        let r = f.accept(c(1, 4, 1, b"BB")); // disagrees on total
        assert!(matches!(
            r,
            AcceptOutcome::Mismatch {
                reason: MismatchReason::TotalPacketsChanged
            }
        ));
    }

    #[test]
    fn layer_change_is_a_mismatch() {
        let mut f = ChunkedFrame::<Chunk>::new();
        f.accept(c(0, 2, 1, b"AA"));
        let r = f.accept(c(1, 2, 2, b"BB")); // different layer
        assert!(matches!(
            r,
            AcceptOutcome::Mismatch {
                reason: MismatchReason::LayerChanged
            }
        ));
    }

    #[test]
    fn packet_no_out_of_range_is_a_mismatch() {
        let mut f = ChunkedFrame::<Chunk>::new();
        let r = f.accept(c(5, 3, 1, b"XX"));
        assert!(matches!(
            r,
            AcceptOutcome::Mismatch {
                reason: MismatchReason::PacketNoOutOfRange
            }
        ));
    }

    #[test]
    fn is_empty_until_first_accept() {
        let mut f = ChunkedFrame::<Chunk>::new();
        assert!(f.is_empty());
        f.accept(c(0, 3, 1, b"AA"));
        assert!(!f.is_empty());
    }
}
