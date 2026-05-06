use std::sync::Mutex;
use crate::node::tcnet_packet::Data;

/// Per-layer pre-built response packets and live state snapshots.
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

impl Default for LayerResponseData {
    fn default() -> Self {
        Self {
            small_waveform_packet: None,
            big_waveform_packets: Vec::new(),
            beat_grid_packets: Vec::new(),
            cue_packet: None,
            artwork_packets: Vec::new(),
            last_metrics: None,
            last_meta: None,
        }
    }
}

pub(crate) struct ResponseDataStore {
    pub layers: Vec<LayerResponseData>,
    pub last_mixer: Option<Data>,
}

impl Default for ResponseDataStore {
    fn default() -> Self {
        let layers = (0..8).map(|_| LayerResponseData::default()).collect();
        Self { layers, last_mixer: None }
    }
}

/// Shared, Mutex-wrapped store passed between ActiveDJNode and Dispatcher.
pub(crate) type SharedResponseData = std::sync::Arc<Mutex<ResponseDataStore>>;
