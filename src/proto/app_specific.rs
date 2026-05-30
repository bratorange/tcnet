//! AppSpecific message reassembly (msg types 30 + 213).
//!
//! `AppSpecificData` (spec page 28) is the vendor-extension carrier:
//! two identifier bytes pick the application, a packet-signature
//! magic guards against malformed wire bytes, and a `(packet_no,
//! total_packets, data_size)` triple lets multi-packet payloads
//! reassemble.
//!
//! Both message-type 30 (broadcast) and 213 (unicast) use the same
//! `AppSpecificData` layout — they differ only in the wire socket
//! the datagram lands on.  Reassembly is identical, so one
//! [`AppSpecificReassembler`] works for both.

use crate::proto::chunked::{AcceptOutcome, ChunkedFrame, ChunkedPayload};
use crate::protocol::{APP_SPECIFIC_SIGNATURE, AppSpecificData};

/// Reason an AppSpecific chunk was rejected before reassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppSpecificError {
    /// `packet_signature` field didn't match
    /// [`APP_SPECIFIC_SIGNATURE`].
    InvalidSignature { got: u32 },
    /// Chunk identifier mismatched an in-flight reassembly.
    IdentifierMismatch { expected: [u8; 2], got: [u8; 2] },
    /// Chunk's claimed `total_packets` disagreed with a prior chunk.
    TotalPacketsChanged,
    /// `packet_no >= total_packets`.
    PacketNoOutOfRange,
}

/// Outcome of feeding one chunk to [`AppSpecificReassembler::accept`].
#[derive(Debug)]
pub enum AppSpecificOutcome {
    /// More chunks needed.
    NeedMore,
    /// Complete payload assembled.
    Complete(AppSpecificFrame),
    /// Duplicate `packet_no` — silently discarded.
    Duplicate,
    /// Validation failure.
    Error(AppSpecificError),
}

/// A fully-reassembled AppSpecific frame, ready for application
/// dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSpecificFrame {
    /// Application identifier carried in `data_identifier_{1,2}`.
    pub identifier: [u8; 2],
    /// Reassembled payload (no signature, no per-chunk headers).
    pub payload: Vec<u8>,
}

/// One in-flight reassembly for a single `(identifier)` pair.  The
/// session task keeps one of these per `(peer, identifier)` while
/// chunks are arriving.
pub struct AppSpecificReassembler {
    identifier: Option<[u8; 2]>,
    frame: ChunkedFrame<ChunkAdapter>,
}

impl Default for AppSpecificReassembler {
    fn default() -> Self {
        Self::new()
    }
}

impl AppSpecificReassembler {
    pub fn new() -> Self {
        Self {
            identifier: None,
            frame: ChunkedFrame::new(),
        }
    }

    /// Feed one freshly-parsed AppSpecificData.  See
    /// [`AppSpecificOutcome`] for return semantics.
    pub fn accept(&mut self, data: AppSpecificData) -> AppSpecificOutcome {
        if data.packet_signature() != APP_SPECIFIC_SIGNATURE {
            return AppSpecificOutcome::Error(AppSpecificError::InvalidSignature {
                got: data.packet_signature(),
            });
        }
        let id = data.identifier();
        if let Some(expected) = self.identifier {
            if expected != id {
                return AppSpecificOutcome::Error(AppSpecificError::IdentifierMismatch {
                    expected,
                    got: id,
                });
            }
        } else {
            self.identifier = Some(id);
        }
        let adapter = ChunkAdapter { inner: data };
        match self.frame.accept(adapter) {
            AcceptOutcome::NeedMore => AppSpecificOutcome::NeedMore,
            AcceptOutcome::Complete(payload) => AppSpecificOutcome::Complete(AppSpecificFrame {
                identifier: self.identifier.unwrap_or([0; 2]),
                payload,
            }),
            AcceptOutcome::Duplicate => AppSpecificOutcome::Duplicate,
            AcceptOutcome::Mismatch { reason } => match reason {
                crate::proto::chunked::MismatchReason::TotalPacketsChanged => {
                    AppSpecificOutcome::Error(AppSpecificError::TotalPacketsChanged)
                }
                crate::proto::chunked::MismatchReason::PacketNoOutOfRange => {
                    AppSpecificOutcome::Error(AppSpecificError::PacketNoOutOfRange)
                }
                crate::proto::chunked::MismatchReason::LayerChanged => {
                    // AppSpecific doesn't use layer; this can't happen.
                    AppSpecificOutcome::NeedMore
                }
            },
        }
    }
}

// AppSpecificData doesn't carry a layer; we set the ChunkedPayload's
// `layer_id` to a fixed 0 sentinel so the layer-mismatch check in the
// generic accumulator never fires (the identifier-mismatch check is
// done in the wrapper above).
struct ChunkAdapter {
    inner: AppSpecificData,
}

impl ChunkedPayload for ChunkAdapter {
    type Assembled = Vec<u8>;
    fn packet_no(&self) -> u32 {
        self.inner.packet_no()
    }
    fn total_packets(&self) -> u32 {
        self.inner.total_packets()
    }
    fn data_size(&self) -> u32 {
        self.inner.data_size()
    }
    fn layer_id(&self) -> u8 {
        0
    }
    fn chunk_bytes(&self) -> &[u8] {
        self.inner.data()
    }
    fn assemble(_layer_id: u8, bytes: Vec<u8>) -> Self::Assembled {
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: [u8; 2], n: u32, total: u32, bytes: &[u8]) -> AppSpecificData {
        AppSpecificData::new_chunk(
            id,
            bytes.to_vec(),
            n,
            total,
            (bytes.len() * total as usize) as u32,
        )
    }

    #[test]
    fn single_packet_payload_completes_immediately() {
        let mut r = AppSpecificReassembler::new();
        let pkt = AppSpecificData::new_single([b'V', b'X'], b"hello".to_vec());
        match r.accept(pkt) {
            AppSpecificOutcome::Complete(f) => {
                assert_eq!(f.identifier, [b'V', b'X']);
                assert_eq!(f.payload, b"hello");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn multi_packet_payload_needs_all_chunks() {
        let mut r = AppSpecificReassembler::new();
        let a = chunk([b'V', b'X'], 0, 3, b"AA");
        let b = chunk([b'V', b'X'], 1, 3, b"BB");
        let c = chunk([b'V', b'X'], 2, 3, b"CC");
        assert!(matches!(r.accept(a), AppSpecificOutcome::NeedMore));
        assert!(matches!(r.accept(b), AppSpecificOutcome::NeedMore));
        match r.accept(c) {
            AppSpecificOutcome::Complete(f) => {
                assert_eq!(f.payload, b"AABBCC");
                assert_eq!(f.identifier, [b'V', b'X']);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bad_signature_is_an_error() {
        let mut r = AppSpecificReassembler::new();
        let mut pkt = AppSpecificData::new_single([0, 0], b"x".to_vec());
        // Mangle the signature by going through the wire round-trip
        // — easiest is to build a chunk with a custom signature
        // value via direct construction.  Since `packet_signature`
        // is private, we round-trip via deku.
        use deku::{DekuContainerRead, DekuContainerWrite};
        let mut bytes = pkt.to_bytes().unwrap();
        // The signature lives at bytes [14..18] in little-endian.
        // 2 (identifier) + 4 (data_size) + 4 (total_packets) + 4 (packet_no) = 14.
        bytes[14..18].copy_from_slice(&0u32.to_le_bytes());
        let (_, mangled) = AppSpecificData::from_bytes((&bytes, 0)).unwrap();
        pkt = mangled;
        match r.accept(pkt) {
            AppSpecificOutcome::Error(AppSpecificError::InvalidSignature { got: 0 }) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn identifier_mismatch_mid_stream_is_an_error() {
        let mut r = AppSpecificReassembler::new();
        r.accept(chunk([b'V', b'X'], 0, 2, b"AA"));
        let bad = chunk([b'V', b'Y'], 1, 2, b"BB");
        match r.accept(bad) {
            AppSpecificOutcome::Error(AppSpecificError::IdentifierMismatch { .. }) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn duplicate_packet_no_is_silently_discarded() {
        let mut r = AppSpecificReassembler::new();
        r.accept(chunk([b'V', b'X'], 0, 2, b"AA"));
        let dup = chunk([b'V', b'X'], 0, 2, b"AA");
        assert!(matches!(r.accept(dup), AppSpecificOutcome::Duplicate));
    }
}
