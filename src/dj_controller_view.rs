use tokio::sync::oneshot;
use crate::node::dj_controller::{DjControllerState, LayerSnapshot, MixerSnapshot, TimeoutError, UserRequest};
use crate::node::tcnet_packet_serde::{ArtworkFileData, BigWaveformData, LayerId, SmallWaveformData};

/// A lightweight, clonable handle that can be sent to background threads to issue
/// waveform requests without needing exclusive access to `DjControllerView`.
pub struct WaveformRequester {
    pub(crate) request_tx: kanal::Sender<UserRequest>,
}

impl Clone for WaveformRequester {
    fn clone(&self) -> Self {
        Self { request_tx: self.request_tx.clone() }
    }
}

const _: fn() = || {
    fn _assert_send<T: Send>() {}
    _assert_send::<WaveformRequester>();
};

impl WaveformRequester {
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
}

/// User-facing read-only view of a discovered foreign DJ controller node.
///
/// Obtained via `TCNetClient::get_controller_view(addr)`.
/// The accessor methods perform a lock-free read from the triple buffer and
/// return a reference into the latest snapshot published by the dispatcher.
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

    /// Returns the latest snapshot of all 8 layers.
    pub fn get_layers(&mut self) -> &[LayerSnapshot] {
        self.buf.read().layers.as_slice()
    }

    /// Returns the latest mixer snapshot.
    pub fn get_mixer(&mut self) -> &MixerSnapshot {
        &self.buf.read().mixer
    }

    /// Request SmallWaveform data for a layer. Times out after 5 seconds.
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

    /// Request BigWaveform data for a layer. Times out after 5 seconds.
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

    /// Returns a clonable handle for issuing waveform requests from background threads.
    pub fn waveform_requester(&self) -> WaveformRequester {
        WaveformRequester { request_tx: self.request_tx.clone() }
    }

    /// Request ArtworkFile data for a layer. Times out after 5 seconds.
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