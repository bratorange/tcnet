//! Domain-layer snapshot types.
//!
//! The session layer publishes peer-identity facts; the domain layer
//! publishes peer-*state* facts decoded from DJ packets and merged
//! across types (Status / Time / Metrics / Meta / Mixer).  Each
//! mergeable field is `Option`-typed until the first relevant packet
//! has arrived, so consumers can distinguish "we haven't seen a BPM
//! yet" from "BPM is exactly 0".

use crate::protocol::{Bpm, LayerState, Speed};

/// Per-layer snapshot, merged across Status / Time / Metrics / Meta
/// packets.
///
/// Fields are `Option`-typed where the spec allows them to be unset
/// until the relevant packet has arrived for that layer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DomainLayerSnapshot {
    /// Layer state — comes from Status packets.  `None` until the
    /// first Status for this layer has been observed.
    pub state: Option<LayerState>,
    /// Current playhead position, milliseconds.  Updated from Time
    /// packets (~20 ms cadence).  `None` until first Time observed.
    pub position_ms: Option<u32>,
    /// Track length, milliseconds.  From Metrics; `None` until first
    /// Metrics packet for this layer.  Notably *does not* become
    /// `Some` from a Time packet alone — Time doesn't carry track
    /// length.
    pub track_length_ms: Option<u32>,
    /// Beat number within the track.  From Metrics.
    pub beat_number: Option<u32>,
    /// Playback speed.  From Metrics.
    pub speed: Option<Speed>,
    /// Master tempo.  From Metrics.
    pub bpm: Option<Bpm>,
    /// Track id.  From Status.
    pub track_id: Option<u32>,
    /// Microsecond timestamp of the most recent packet that touched
    /// any field on this layer — used by [`TimestampOrdered`] to
    /// reject stale observations.
    pub last_updated_us: u32,
}

impl DomainLayerSnapshot {
    /// Has at least one packet been observed for this layer?
    pub fn has_been_observed(&self) -> bool {
        self.state.is_some()
            || self.position_ms.is_some()
            || self.track_length_ms.is_some()
            || self.beat_number.is_some()
            || self.bpm.is_some()
            || self.track_id.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_none() {
        let s = DomainLayerSnapshot::default();
        assert!(s.state.is_none());
        assert!(s.position_ms.is_none());
        assert!(s.track_length_ms.is_none());
        assert!(s.bpm.is_none());
        assert!(s.track_id.is_none());
        assert!(!s.has_been_observed());
    }

    #[test]
    fn has_been_observed_after_any_field_set() {
        let mut s = DomainLayerSnapshot::default();
        s.state = Some(LayerState::Playing);
        assert!(s.has_been_observed());
    }
}
