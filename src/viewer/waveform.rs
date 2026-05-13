use std::sync::{Arc, mpsc};
use crate::node::tcnet_packet_serde::LayerId;
use crate::{BigWaveformData, SmallWaveformData, WaveformRequester};

pub enum WaveformResult {
    Small(usize, Arc<SmallWaveformData>),
    Big(usize, Arc<BigWaveformData>),
    Failed(usize),
}

pub struct WaveformCache {
    pub small: [Option<Arc<SmallWaveformData>>; 4],
    pub big: [Option<Arc<BigWaveformData>>; 4],
    last_track_id: [u32; 4],
    pending_small: [bool; 4],
    pending_big: [bool; 4],
    result_tx: mpsc::SyncSender<WaveformResult>,
    result_rx: mpsc::Receiver<WaveformResult>,
}

impl WaveformCache {
    pub fn new() -> Self {
        let (result_tx, result_rx) = mpsc::sync_channel(32);
        Self {
            small: [None, None, None, None],
            big: [None, None, None, None],
            last_track_id: [0; 4],
            pending_small: [false; 4],
            pending_big: [false; 4],
            result_tx,
            result_rx,
        }
    }

    pub fn update(
        &mut self,
        deck_idx: usize,
        track_id: u32,
        layer_id: LayerId,
        requester: &WaveformRequester,
    ) {
        if track_id == 0 || track_id == self.last_track_id[deck_idx] {
            return;
        }
        self.last_track_id[deck_idx] = track_id;
        self.small[deck_idx] = None;
        self.big[deck_idx] = None;
        self.pending_small[deck_idx] = true;
        self.pending_big[deck_idx] = true;

        {
            let req = requester.clone();
            let tx = self.result_tx.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio rt");
                match rt.block_on(req.request_small_waveform(layer_id)) {
                    Ok(data) => { tx.send(WaveformResult::Small(deck_idx, Arc::new(data))).ok(); }
                    Err(_) => { tx.send(WaveformResult::Failed(deck_idx)).ok(); }
                }
            });
        }

        {
            let req = requester.clone();
            let tx = self.result_tx.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio rt");
                match rt.block_on(req.request_big_waveform(layer_id)) {
                    Ok(data) => { tx.send(WaveformResult::Big(deck_idx, Arc::new(data))).ok(); }
                    Err(_) => { tx.send(WaveformResult::Failed(deck_idx)).ok(); }
                }
            });
        }
    }

    pub fn poll(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                WaveformResult::Small(idx, data) => {
                    self.small[idx] = Some(data);
                    self.pending_small[idx] = false;
                }
                WaveformResult::Big(idx, data) => {
                    self.big[idx] = Some(data);
                    self.pending_big[idx] = false;
                }
                WaveformResult::Failed(idx) => {
                    self.pending_small[idx] = false;
                    self.pending_big[idx] = false;
                    self.last_track_id[idx] = 0; // allow retry on next frame
                }
            }
        }
    }
}
