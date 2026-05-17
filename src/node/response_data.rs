use crate::node::tcnet_packet::Data;
use std::sync::Mutex;

/// Per-layer pre-built response packets and live state snapshots.
#[derive(Default)]
pub(crate) struct LayerResponseData {
    pub small_waveform_packet: Option<Data>,
    pub big_waveform_packets: Vec<Data>,
    pub beat_grid_packets: Vec<Data>,
    pub cue_packet: Option<Data>,
    pub artwork_packets: Vec<Data>,
    /// Last-sent MetricsData — returned verbatim on MetricsData requests.
    pub last_metrics: Option<Data>,
    /// Last-sent MetaData — returned verbatim on MetaData requests.
    pub last_meta: Option<Data>,
}

pub(crate) struct ResponseDataStore {
    pub layers: Vec<LayerResponseData>,
    pub last_mixer: Option<Data>,
}

impl Default for ResponseDataStore {
    fn default() -> Self {
        let layers = (0..8).map(|_| LayerResponseData::default()).collect();
        Self {
            layers,
            last_mixer: None,
        }
    }
}

/// Shared, Mutex-wrapped store passed between ActiveDJNode and Dispatcher.
pub(crate) type SharedResponseData = std::sync::Arc<Mutex<ResponseDataStore>>;
