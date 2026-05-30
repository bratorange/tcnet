use crate::node::tcnet_packet::Data;
use arc_swap::ArcSwap;
use std::sync::Arc;

/// Per-layer pre-built response packets and live state snapshots.
///
/// Each field is independently swappable through `ArcSwap`, so the
/// hot path (Request handler in the dispatcher, periodic update tasks
/// in the active node) never touches a mutex.
pub(crate) struct LayerResponseData {
    pub small_waveform_packet: ArcSwap<Option<Data>>,
    pub big_waveform_packets: ArcSwap<Vec<Data>>,
    pub beat_grid_packets: ArcSwap<Vec<Data>>,
    pub cue_packet: ArcSwap<Option<Data>>,
    pub artwork_packets: ArcSwap<Vec<Data>>,
    /// Last-sent MetricsData — returned verbatim on MetricsData requests.
    pub last_metrics: ArcSwap<Option<Data>>,
    /// Last-sent MetaData — returned verbatim on MetaData requests.
    pub last_meta: ArcSwap<Option<Data>>,
}

impl Default for LayerResponseData {
    fn default() -> Self {
        Self {
            small_waveform_packet: ArcSwap::from_pointee(None),
            big_waveform_packets: ArcSwap::from_pointee(Vec::new()),
            beat_grid_packets: ArcSwap::from_pointee(Vec::new()),
            cue_packet: ArcSwap::from_pointee(None),
            artwork_packets: ArcSwap::from_pointee(Vec::new()),
            last_metrics: ArcSwap::from_pointee(None),
            last_meta: ArcSwap::from_pointee(None),
        }
    }
}

pub(crate) struct ResponseDataStore {
    pub layers: [LayerResponseData; 8],
    pub last_mixer: ArcSwap<Option<Data>>,
}

impl Default for ResponseDataStore {
    fn default() -> Self {
        Self {
            layers: std::array::from_fn(|_| LayerResponseData::default()),
            last_mixer: ArcSwap::from_pointee(None),
        }
    }
}

/// Shared lock-free response store passed between ActiveDJNode and Dispatcher.
pub(crate) type SharedResponseData = Arc<ResponseDataStore>;
