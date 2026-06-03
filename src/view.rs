//! Per-peer read view of a foreign TCNet DJ controller.
//!
//! Returned by [`Node::layers_for`](crate::api::Node::layers_for) /
//! [`Node::mixer_for`](crate::api::Node::mixer_for) under the hood;
//! also surfaced via [`Node::waveform_requester_for`](crate::api::Node::waveform_requester_for)
//! when callers need a `'static` request handle for background tasks.
//!
//! Reads are wait-free via `Arc<ArcSwap<DjControllerState>>`; on-demand
//! requests for waveform / beat-grid / artwork data are async and time
//! out after 5 s.

use crate::node::dj_controller::{
    DjControllerState, LayerSnapshot, MixerSnapshot, TimeoutError, UserRequest,
};
use crate::protocol::{
    ArtworkFileData, BeatGridHeader, BigWaveformData, CueData, LayerId, SmallWaveformData,
};
use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Send-clonable handle for issuing waveform / beat-grid / artwork requests
/// from background threads.
///
/// Obtained via [`Node::waveform_requester_for`](crate::api::Node::waveform_requester_for).
/// Each request method returns a future that resolves with the response data
/// or [`TimeoutError`] if no response is received within 5 s.
pub struct WaveformRequester {
    pub(crate) request_tx: kanal::Sender<UserRequest>,
}

impl Clone for WaveformRequester {
    fn clone(&self) -> Self {
        Self {
            request_tx: self.request_tx.clone(),
        }
    }
}

const _: fn() = || {
    fn _assert_send<T: Send>() {}
    _assert_send::<WaveformRequester>();
};

impl WaveformRequester {
    /// Request the small (2400-byte) waveform for `layer`. Times out after 5 s.
    pub async fn request_small_waveform(
        &self,
        layer: LayerId,
    ) -> Result<SmallWaveformData, TimeoutError> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send(UserRequest::SmallWaveform { layer, reply: tx })
            .map_err(|_| TimeoutError)?;
        rx.await.map_err(|_| TimeoutError)?
    }

    /// Request the high-resolution waveform for `layer`. Times out after 5 s.
    pub async fn request_big_waveform(
        &self,
        layer: LayerId,
    ) -> Result<BigWaveformData, TimeoutError> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send(UserRequest::BigWaveform { layer, reply: tx })
            .map_err(|_| TimeoutError)?;
        rx.await.map_err(|_| TimeoutError)?
    }

    /// Request the beat grid for `layer`. The multi-packet response is
    /// reassembled internally before the future resolves. Times out after 5 s.
    pub async fn request_beat_grid(&self, layer: LayerId) -> Result<BeatGridHeader, TimeoutError> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send(UserRequest::BeatGrid { layer, reply: tx })
            .map_err(|_| TimeoutError)?;
        rx.await.map_err(|_| TimeoutError)?
    }

    /// Request the full cue table (memory cue + 17 hot-cue slots + loop
    /// in/out) for `layer`. Times out after 5 s.
    pub async fn request_cue_data(&self, layer: LayerId) -> Result<CueData, TimeoutError> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send(UserRequest::CueData { layer, reply: tx })
            .map_err(|_| TimeoutError)?;
        rx.await.map_err(|_| TimeoutError)?
    }

    /// Request the low-resolution artwork JPEG for `layer`. The multi-packet
    /// response is reassembled internally. Times out after 5 s.
    pub async fn request_artwork_file(
        &self,
        layer: LayerId,
    ) -> Result<ArtworkFileData, TimeoutError> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send(UserRequest::ArtworkFile { layer, reply: tx })
            .map_err(|_| TimeoutError)?;
        rx.await.map_err(|_| TimeoutError)?
    }
}


/// Internal per-peer read handle. One is cached per discovered node inside
/// [`Node`](crate::Node), which is the public surface; callers reach this
/// view's accessors through [`Node::layers_for`](crate::Node::layers_for),
/// [`Node::mixer_for`](crate::Node::mixer_for), the `Node::request_*` methods,
/// and [`Node::waveform_requester_for`](crate::Node::waveform_requester_for).
///
/// The state accessors ([`get_layers`](Self::get_layers),
/// [`get_mixer`](Self::get_mixer)) read from a lock-free triple buffer that
/// the dispatcher updates as packets arrive — they always return the most
/// recent snapshot. On-demand requests go through the
/// [`WaveformRequester`](Self::waveform_requester) handle.
pub(crate) struct DjControllerView {
    state: Arc<ArcSwap<DjControllerState>>,
    /// Cached `load_full()` from the latest accessor call so we can
    /// hand out `&LayerSnapshot` / `&MixerSnapshot` with a stable
    /// lifetime tied to `&mut self`.  Refreshed on every accessor
    /// call via [`Self::refresh`].
    cached: Arc<DjControllerState>,
    request_tx: kanal::Sender<UserRequest>,
}

impl Clone for DjControllerView {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            cached: self.cached.clone(),
            request_tx: self.request_tx.clone(),
        }
    }
}

impl DjControllerView {
    pub(crate) fn new(
        state: Arc<ArcSwap<DjControllerState>>,
        request_tx: kanal::Sender<UserRequest>,
    ) -> Self {
        let cached = state.load_full();
        Self {
            state,
            cached,
            request_tx,
        }
    }

    fn refresh(&mut self) {
        self.cached = self.state.load_full();
    }

    /// Latest snapshot of the eight layer states, in [`LayerId::ALL`] order.
    pub fn get_layers(&mut self) -> &[LayerSnapshot] {
        self.refresh();
        self.cached.layers.as_slice()
    }

    /// Latest mixer snapshot.
    pub fn get_mixer(&mut self) -> &MixerSnapshot {
        self.refresh();
        &self.cached.mixer
    }

    /// Return a Send + clonable [`WaveformRequester`] for issuing waveform /
    /// beat-grid / cue / artwork requests from background threads without
    /// holding `&mut self`.
    pub fn waveform_requester(&self) -> WaveformRequester {
        WaveformRequester {
            request_tx: self.request_tx.clone(),
        }
    }
}
