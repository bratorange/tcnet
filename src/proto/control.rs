//! Typed builder for TCNet Control-message paths (V3.5.1B msg type 101).
//!
//! The wire layer carries `ControlData { step, control_path: Vec<u8> }` —
//! an opaque ASCII string.  The actual *meaning* of the string is
//! application-defined.  This module standardises a small set of
//! conventions for well-known operations (set layer state, set layer
//! source) and offers a `raw` escape hatch for vendor-specific paths.
//!
//! The convention follows a URL-like path-and-query shape:
//!
//! ```text
//!   layer/2/state=3
//!   layer/A/source=1
//!   mixer/master/level=120
//! ```
//!
//! Phase 8 plumbs this through a typed `Node<Master|Auto, V>` send
//! method that demands an `Authenticated` peer witness — until then,
//! the builder is callable in isolation and the sender gates auth.

use crate::protocol::{ControlData, LayerId, LayerState};

/// One TCNet Control path, ready for embedding in [`ControlData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlPath {
    bytes: Vec<u8>,
}

/// Source identifier — what's playing on a given layer.
///
/// The wire format quotes a single ASCII digit / letter; this enum
/// captures the canonical values plus a fallback for vendor-defined
/// extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceId {
    Layer1,
    Layer2,
    Layer3,
    Layer4,
    LayerA,
    LayerB,
    LayerM,
    LayerC,
    /// Vendor-specific source id — single ASCII byte.
    Other(u8),
}

impl SourceId {
    /// Single-byte ASCII representation used in control-path strings.
    pub fn as_ascii_byte(self) -> u8 {
        match self {
            Self::Layer1 => b'1',
            Self::Layer2 => b'2',
            Self::Layer3 => b'3',
            Self::Layer4 => b'4',
            Self::LayerA => b'A',
            Self::LayerB => b'B',
            Self::LayerM => b'M',
            Self::LayerC => b'C',
            Self::Other(b) => b,
        }
    }
}

impl ControlPath {
    /// Escape hatch — wrap an arbitrary ASCII string.  Non-ASCII
    /// bytes are passed through unchanged; consumers that need
    /// validation should call [`ControlPath::is_ascii`] first.
    pub fn raw(s: impl Into<String>) -> Self {
        Self {
            bytes: s.into().into_bytes(),
        }
    }

    /// Build a raw path from already-prepared bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// `layer/<id>/state=<n>` — request a layer state change.
    pub fn set_layer_state(layer: LayerId, state: LayerState) -> Self {
        let s = format!(
            "layer/{}/state={}",
            layer_label(layer),
            layer_state_byte(state)
        );
        Self::raw(s)
    }

    /// `layer/<id>/source=<src>` — request a layer source change.
    pub fn set_layer_source(layer: LayerId, source: SourceId) -> Self {
        let s = format!(
            "layer/{}/source={}",
            layer_label(layer),
            source.as_ascii_byte() as char
        );
        Self::raw(s)
    }

    /// `mixer/master/level=<v>` — set the master output level.
    pub fn set_master_level(level: u8) -> Self {
        Self::raw(format!("mixer/master/level={}", level))
    }

    /// All-ASCII check (`0x20..=0x7E`).
    pub fn is_ascii(&self) -> bool {
        self.bytes.iter().all(|b| (0x20..=0x7E).contains(b))
    }

    /// View as `&str` if all-ASCII; otherwise `None`.
    pub fn as_str(&self) -> Option<&str> {
        if self.is_ascii() {
            // SAFETY: all bytes are printable ASCII.
            Some(unsafe { std::str::from_utf8_unchecked(&self.bytes) })
        } else {
            None
        }
    }

    /// Raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Wrap as an outgoing `ControlData` (step = 0).
    pub fn into_initial(self) -> ControlData {
        ControlData::new_initial(self.bytes)
    }

    /// Wrap as a response `ControlData` (step = 1).
    pub fn into_response(self) -> ControlData {
        ControlData::new_response(self.bytes)
    }
}

fn layer_label(layer: LayerId) -> &'static str {
    match layer {
        LayerId::L1 => "1",
        LayerId::L2 => "2",
        LayerId::L3 => "3",
        LayerId::L4 => "4",
        LayerId::LA => "A",
        LayerId::LB => "B",
        LayerId::LM => "M",
        LayerId::LC => "C",
    }
}

fn layer_state_byte(state: LayerState) -> u8 {
    match state {
        LayerState::Idle => 0,
        LayerState::Playing => 3,
        LayerState::Looping => 4,
        LayerState::Paused => 5,
        LayerState::Stopped => 6,
        LayerState::CueButtonDown => 7,
        LayerState::PlatterDown => 8,
        LayerState::FastForward => 9,
        LayerState::FastReverse => 10,
        LayerState::Hold => 11,
        LayerState::Unknown(b) => b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_round_trips_ascii() {
        let p = ControlPath::raw("layer/1/state=3");
        assert!(p.is_ascii());
        assert_eq!(p.as_str(), Some("layer/1/state=3"));
        assert_eq!(p.as_bytes(), b"layer/1/state=3");
    }

    #[test]
    fn set_layer_state_emits_canonical_form() {
        let p = ControlPath::set_layer_state(LayerId::L2, LayerState::Playing);
        let s = p.as_str().unwrap();
        // LayerState::Playing == 3 per protocol.rs definition.
        assert_eq!(s, "layer/2/state=3");
    }

    #[test]
    fn set_layer_source_uses_ascii_letter_for_named_layers() {
        let p = ControlPath::set_layer_source(LayerId::LA, SourceId::LayerB);
        assert_eq!(p.as_str(), Some("layer/A/source=B"));
    }

    #[test]
    fn set_master_level_path() {
        let p = ControlPath::set_master_level(120);
        assert_eq!(p.as_str(), Some("mixer/master/level=120"));
    }

    #[test]
    fn source_id_other_passes_through_byte() {
        assert_eq!(SourceId::Other(b'X').as_ascii_byte(), b'X');
        let p = ControlPath::set_layer_source(LayerId::LM, SourceId::Other(b'X'));
        assert_eq!(p.as_str(), Some("layer/M/source=X"));
    }

    #[test]
    fn into_initial_builds_step_zero_control_data() {
        let p = ControlPath::raw("layer/1/state=3");
        let cd = p.into_initial();
        assert_eq!(cd.step(), 0);
        assert_eq!(cd.control_path(), b"layer/1/state=3");
    }

    #[test]
    fn into_response_builds_step_one_control_data() {
        let p = ControlPath::raw("ack");
        let cd = p.into_response();
        assert_eq!(cd.step(), 1);
        assert_eq!(cd.control_path(), b"ack");
    }

    #[test]
    fn non_ascii_path_reports_correctly() {
        let p = ControlPath::from_bytes(vec![0xff, 0xfe]);
        assert!(!p.is_ascii());
        assert!(p.as_str().is_none());
    }
}
