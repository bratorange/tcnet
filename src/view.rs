//! Read-only consumer view of a foreign TCNet DJ controller node.
//!
//! Obtain a [`DjControllerView`] via
//! [`TCNetClient::get_controller_view`](crate::TCNetClient::get_controller_view).
//! Reads are lock-free (backed by a triple buffer); on-demand requests for
//! waveform / beat-grid / artwork data are async and time out after 5 seconds.

use crate::node::dj_controller::{
    DjControllerState, LayerSnapshot, MixerSnapshot, TimeoutError, UserRequest,
};
use crate::protocol::{
    ArtworkFileData, BeatGridHeader, BigWaveformData, CueData, LayerId, SmallWaveformData,
};
use tokio::sync::oneshot;

/// Send-clonable handle for issuing waveform / beat-grid / artwork requests
/// from background threads.
///
/// Obtained via [`DjControllerView::waveform_requester`]. Each request method
/// returns a future that resolves with the response data or [`TimeoutError`]
/// if no response is received within 5 s.
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
}


/// Read-only view of a discovered foreign TCNet DJ controller node.
///
/// Obtained via
/// [`TCNetClient::get_controller_view`](crate::TCNetClient::get_controller_view).
/// The state accessors ([`get_layers`](Self::get_layers),
/// [`get_mixer`](Self::get_mixer)) read from a lock-free triple buffer that
/// the dispatcher updates as packets arrive — they always return the most
/// recent snapshot.
///
/// On-demand request methods ([`request_small_waveform`](Self::request_small_waveform),
/// [`request_big_waveform`](Self::request_big_waveform),
/// [`request_beat_grid`](Self::request_beat_grid),
/// [`request_artwork_file`](Self::request_artwork_file)) issue a
/// [`RequestData`](crate::protocol::RequestData) over the wire and wait for
/// the reply; each times out after 5 s with [`TimeoutError`].
pub struct DjControllerView {
    buf: triple_buffer::Output<DjControllerState>,
    request_tx: kanal::Sender<UserRequest>,
}

impl DjControllerView {
    pub(crate) fn new(
        buf: triple_buffer::Output<DjControllerState>,
        request_tx: kanal::Sender<UserRequest>,
    ) -> Self {
        Self { buf, request_tx }
    }

    /// Latest snapshot of the eight layer states, in [`LayerId::ALL`] order.
    pub fn get_layers(&mut self) -> &[LayerSnapshot] {
        self.buf.read().layers.as_slice()
    }

    /// Latest mixer snapshot.
    pub fn get_mixer(&mut self) -> &MixerSnapshot {
        &self.buf.read().mixer
    }

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

    /// Request the high-resolution waveform for `layer`. The multi-packet
    /// response is reassembled internally. Times out after 5 s.
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
    /// reassembled into a single [`BeatGridHeader`] (with the full payload in
    /// its `payload` field) before the future resolves. Times out after 5 s.
    pub async fn request_beat_grid(&self, layer: LayerId) -> Result<BeatGridHeader, TimeoutError> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send(UserRequest::BeatGrid { layer, reply: tx })
            .map_err(|_| TimeoutError)?;
        rx.await.map_err(|_| TimeoutError)?
    }

    /// Request the full cue table for `layer`. Times out after 5 s.
    pub async fn request_cue_data(&self, layer: LayerId) -> Result<CueData, TimeoutError> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send(UserRequest::CueData { layer, reply: tx })
            .map_err(|_| TimeoutError)?;
        rx.await.map_err(|_| TimeoutError)?
    }

    /// Return a Send + clonable handle for issuing waveform / beat-grid /
    /// artwork requests from background threads without holding `&mut self`.
    pub fn waveform_requester(&self) -> WaveformRequester {
        WaveformRequester {
            request_tx: self.request_tx.clone(),
        }
    }

    /// Request the low-resolution artwork JPEG for `layer`. Times out after 5 s.
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
