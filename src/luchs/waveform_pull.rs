use std::sync::Arc;

use crate::{BeatGridEntry, LayerId, WaveformRequester};
use deku::DekuContainerRead;

const NUM_DECKS: usize = 4;

#[derive(Debug, Clone)]
pub enum WaveformEvent {
    Small {
        deck_idx: usize,
        track_id: u32,
        bytes: Arc<Vec<u8>>,
    },
    Big {
        deck_idx: usize,
        track_id: u32,
        bytes: Arc<Vec<u8>>,
    },
    BeatGrid {
        deck_idx: usize,
        track_id: u32,
        entries: Arc<Vec<BeatGridEntry>>,
    },
    /// Reset stored waveforms when track changes or unloads.
    Cleared {
        deck_idx: usize,
    },
}

#[derive(Default)]
struct DeckPending {
    last_track_id: u32,
    small_inflight: bool,
    big_inflight: bool,
    grid_inflight: bool,
}

pub struct WaveformPuller {
    rt: tokio::runtime::Handle,
    event_tx: kanal::Sender<WaveformEvent>,
    event_rx: kanal::Receiver<WaveformEvent>,
    pending: [DeckPending; NUM_DECKS],
}

impl WaveformPuller {
    pub fn new(rt: tokio::runtime::Handle) -> Self {
        let (event_tx, event_rx) = kanal::bounded::<WaveformEvent>(64);
        Self {
            rt,
            event_tx,
            event_rx,
            pending: Default::default(),
        }
    }

    /// Call once per deck per frame. Issues pending requests on track-id
    /// transitions and clears state when the deck is unloaded.
    pub fn refresh(
        &mut self,
        deck_idx: usize,
        track_id: u32,
        layer_id: LayerId,
        requester: &WaveformRequester,
    ) {
        if deck_idx >= NUM_DECKS {
            return;
        }
        let state = &mut self.pending[deck_idx];

        if track_id != state.last_track_id {
            state.last_track_id = track_id;
            state.small_inflight = false;
            state.big_inflight = false;
            state.grid_inflight = false;
            let _ = self.event_tx.try_send(WaveformEvent::Cleared { deck_idx });
            if track_id == 0 {
                // Unload — nothing to request.
                return;
            }
        }

        if track_id == 0 {
            return;
        }

        if !state.small_inflight {
            state.small_inflight = true;
            spawn_small(&self.rt, deck_idx, track_id, layer_id, requester.clone(), self.event_tx.clone());
        }
        if !state.big_inflight {
            state.big_inflight = true;
            spawn_big(&self.rt, deck_idx, track_id, layer_id, requester.clone(), self.event_tx.clone());
        }
        if !state.grid_inflight {
            state.grid_inflight = true;
            spawn_grid(&self.rt, deck_idx, track_id, layer_id, requester.clone(), self.event_tx.clone());
        }
    }

    /// Drain all completed events. Caller should merge into DeckState.
    pub fn drain_events(&self) -> Vec<WaveformEvent> {
        let mut out = Vec::new();
        let _ = self.event_rx.drain_into(&mut out);
        out
    }
}

fn spawn_small(
    rt: &tokio::runtime::Handle,
    deck_idx: usize,
    track_id: u32,
    layer: LayerId,
    req: WaveformRequester,
    tx: kanal::Sender<WaveformEvent>,
) {
    rt.spawn(async move {
        match req.request_small_waveform(layer).await {
            Ok(data) => {
                let bytes = Arc::new(data.bytes().to_vec());
                let _ = tx.try_send(WaveformEvent::Small { deck_idx, track_id, bytes });
            }
            Err(_) => {
                log::debug!("small waveform request timeout for deck {}", deck_idx);
            }
        }
    });
}

fn spawn_big(
    rt: &tokio::runtime::Handle,
    deck_idx: usize,
    track_id: u32,
    layer: LayerId,
    req: WaveformRequester,
    tx: kanal::Sender<WaveformEvent>,
) {
    rt.spawn(async move {
        match req.request_big_waveform(layer).await {
            Ok(data) => {
                let bytes = Arc::new(data.bytes().to_vec());
                let _ = tx.try_send(WaveformEvent::Big { deck_idx, track_id, bytes });
            }
            Err(_) => {
                log::debug!("big waveform request timeout for deck {}", deck_idx);
            }
        }
    });
}

fn spawn_grid(
    rt: &tokio::runtime::Handle,
    deck_idx: usize,
    track_id: u32,
    layer: LayerId,
    req: WaveformRequester,
    tx: kanal::Sender<WaveformEvent>,
) {
    rt.spawn(async move {
        match req.request_beat_grid(layer).await {
            Ok(header) => {
                let entries = parse_beat_grid(&header.payload);
                if entries.is_empty() {
                    log::warn!(
                        "luchs: beat-grid arrived empty for deck {} track {} ({} payload bytes)",
                        deck_idx, track_id, header.payload.len()
                    );
                } else {
                    log::info!(
                        "luchs: beat-grid arrived for deck {} track {} — {} entries from {} bytes. \
                         first={:?} last={:?} downbeats={}",
                        deck_idx, track_id, entries.len(), header.payload.len(),
                        entries.first().map(|e| (e.beat_number, e.beat_type, e.beat_timestamp)),
                        entries.last().map(|e| (e.beat_number, e.beat_type, e.beat_timestamp)),
                        entries.iter().filter(|e| e.beat_type == 20).count(),
                    );
                }
                let _ = tx.try_send(WaveformEvent::BeatGrid {
                    deck_idx,
                    track_id,
                    entries: Arc::new(entries),
                });
            }
            Err(_) => {
                log::warn!(
                    "luchs: beat grid request TIMED OUT for deck {} track {}",
                    deck_idx, track_id
                );
            }
        }
    });
}

fn parse_beat_grid(payload: &[u8]) -> Vec<BeatGridEntry> {
    let mut out = Vec::with_capacity(payload.len() / 8);
    let mut remaining = payload;
    while remaining.len() >= 8 {
        match BeatGridEntry::from_bytes((remaining, 0)) {
            Ok(((rest, _), entry)) => {
                out.push(entry);
                remaining = rest;
            }
            Err(_) => break,
        }
    }
    out
}
