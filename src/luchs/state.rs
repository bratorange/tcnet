use std::time::Instant;

use crate::media_library::VirtualUsb;
use crate::{DjControllerView, LayerId};

use super::analysis::{AnalysisEvent, AnalysisManager, AnalysisPriority};
use super::deck_state::{DeckRole, DeckState};
use super::osc::{OscEvent, OscSender};
use super::phrase_types::AnalysisState;
use super::waveform_pull::{WaveformEvent, WaveformPuller};

pub struct LuchsState {
    pub decks: [DeckState; 4],
    pub connected: bool,
    pub player_count: usize,
}

impl Default for LuchsState {
    fn default() -> Self {
        Self {
            decks: [
                DeckState::new(1),
                DeckState::new(2),
                DeckState::new(3),
                DeckState::new(4),
            ],
            connected: false,
            player_count: 0,
        }
    }
}

const DECK_LAYER_IDS: [LayerId; 4] = [LayerId::L1, LayerId::L2, LayerId::L3, LayerId::L4];

impl LuchsState {
    /// Drain the latest snapshot from the view, update each deck, pump the
    /// waveform/beat-grid request pipeline, submit / merge analysis jobs, and
    /// emit OSC events for phrase / beat changes.
    pub fn refresh(
        &mut self,
        view: &mut DjControllerView,
        puller: &mut WaveformPuller,
        analysis: &mut AnalysisManager,
        library: &VirtualUsb,
        osc: &OscSender,
        forward_all_decks: bool,
    ) {
        let now = Instant::now();
        let layers: Vec<_> = view.get_layers().iter().take(4).cloned().collect();
        let roles = super::on_air::compute_roles(&layers);

        // Track which decks' track_id changed this frame (so we can clear
        // analysis state alongside waveform state).
        let mut track_changed = [false; 4];
        for (i, snap) in layers.iter().enumerate() {
            track_changed[i] = self.decks[i].snap.track_id != snap.track_id;
        }

        // Apply snapshot + role.
        for (i, snap) in layers.iter().enumerate() {
            self.decks[i].ingest_snapshot(snap.clone(), now);
            self.decks[i].role = roles[i];
        }

        // Clear analysis state for any deck whose track changed.
        for (i, changed) in track_changed.iter().enumerate() {
            if *changed {
                self.decks[i].clear_analysis();
                analysis.clear(i);
            }
        }

        // Issue waveform/beat-grid requests for any deck whose track_id changed.
        let requester = view.waveform_requester();
        for (i, snap) in layers.iter().enumerate() {
            puller.refresh(i, snap.track_id, DECK_LAYER_IDS[i], &requester);
        }

        // Merge completed waveform events into deck state.
        for event in puller.drain_events() {
            self.apply_waveform_event(event);
        }

        // Submit analysis jobs for decks with a resolvable audio file.
        for (i, snap) in layers.iter().enumerate() {
            if snap.track_id == 0 {
                continue;
            }
            // Resolve once and cache on the deck. Match against the indexed
            // media library by (title, artist) — same primary keys the
            // simulator uses to broadcast a track, so the match is exact
            // when the same tags are present in the local files.
            if self.decks[i].audio_path.is_none() && !self.decks[i].audio_path_missing {
                let title = if !snap.title.is_empty() {
                    snap.title.as_str()
                } else if !snap.name.is_empty() {
                    snap.name.as_str()
                } else {
                    continue;
                };
                let artist = snap.artist.as_str();
                match library.lookup(title, artist) {
                    Some(info) => {
                        self.decks[i].audio_path = Some(info.path.clone());
                    }
                    None => {
                        log::warn!(
                            "luchs: no audio file for track title={:?} artist={:?} in library root={:?}",
                            title,
                            artist,
                            library.root,
                        );
                        self.decks[i].audio_path_missing = true;
                    }
                }
            }

            if let Some(path) = self.decks[i].audio_path.clone() {
                let priority = match self.decks[i].role {
                    DeckRole::OnAir => AnalysisPriority::OnAir,
                    DeckRole::Next => AnalysisPriority::Next,
                    _ => AnalysisPriority::Idle,
                };
                let prev_state = matches!(
                    self.decks[i].analysis_state,
                    AnalysisState::NotStarted | AnalysisState::Failed { .. }
                );
                if prev_state {
                    self.decks[i].analysis_state = AnalysisState::Queued;
                }
                // Cache key is derived from title+artist (metadata-based) so
                // it's portable across machines / file moves.
                let title = if !snap.title.is_empty() {
                    snap.title.clone()
                } else {
                    snap.name.clone()
                };
                analysis.submit(i, snap.track_id, path, title, snap.artist.clone(), priority);
            }
        }

        // Merge analysis events.
        for event in analysis.drain_events() {
            self.apply_analysis_event(event);
        }

        // Emit OSC events for any deck that just crossed a beat or a phrase
        // boundary. By default we only forward the on-air deck; the user can
        // opt into forwarding everything via the settings checkbox.
        for i in 0..self.decks.len() {
            let should_forward = forward_all_decks
                || matches!(self.decks[i].role, DeckRole::OnAir);
            if !should_forward {
                continue;
            }
            self.emit_osc_for_deck(i, osc);
        }
    }

    fn emit_osc_for_deck(&mut self, deck_idx: usize, osc: &OscSender) {
        let Some(deck) = self.decks.get_mut(deck_idx) else {
            return;
        };

        // Beat event — fires when the local beat counter advances. We prefer
        // the TCNet snapshot's beat_number when available, but fall back to a
        // BPM-derived counter (1-based) so OSC still ticks when the bridge
        // doesn't supply beat_number (the simulator currently doesn't).
        let snap_beat = deck.snap.beat_number;
        let bpm = deck.snap.bpm.as_f32();
        let derived_beat = if bpm > 1.0 {
            ((deck.predicted_position_ms as f32) / (60_000.0 / bpm)) as u32 + 1
        } else {
            0
        };
        let new_beat = snap_beat.max(derived_beat);
        if new_beat > 0 && new_beat != deck.last_emitted_beat {
            osc.dispatch(OscEvent::Beat {
                beat_number: new_beat,
            });
            deck.last_emitted_beat = new_beat;
        }

        // Phrase event — fires when entering a new segment (verse→verse also
        // fires per spec, since segment_idx changes).
        if let Some((seg_idx, seg)) = deck.current_segment_with_index() {
            let key = (seg.kind, seg_idx);
            if deck.last_emitted_segment != Some(key) {
                osc.dispatch(OscEvent::Phrase {
                    segment_idx: seg_idx as i32,
                });
                deck.last_emitted_segment = Some(key);
            }
        }
    }

    fn apply_waveform_event(&mut self, event: WaveformEvent) {
        match event {
            WaveformEvent::Cleared { deck_idx } => {
                if let Some(deck) = self.decks.get_mut(deck_idx) {
                    deck.clear_waveform();
                }
            }
            WaveformEvent::Small { deck_idx, track_id, bytes } => {
                if let Some(deck) = self.decks.get_mut(deck_idx) {
                    if deck.snap.track_id == track_id {
                        deck.small_waveform_bytes = Some(bytes);
                    }
                }
            }
            WaveformEvent::Big { deck_idx, track_id, bytes } => {
                if let Some(deck) = self.decks.get_mut(deck_idx) {
                    if deck.snap.track_id == track_id {
                        deck.big_waveform_bytes = Some(bytes);
                    }
                }
            }
            WaveformEvent::BeatGrid { deck_idx, track_id, entries } => {
                if let Some(deck) = self.decks.get_mut(deck_idx) {
                    if deck.snap.track_id == track_id {
                        deck.beat_grid = Some(entries);
                    }
                }
            }
        }
    }

    fn apply_analysis_event(&mut self, event: AnalysisEvent) {
        match event {
            AnalysisEvent::McPitchReady {
                deck_idx,
                track_id,
                mp,
                pitch,
            } => {
                if let Some(deck) = self.decks.get_mut(deck_idx) {
                    if deck.snap.track_id == track_id {
                        deck.mp_curve = Some(mp);
                        deck.pitch_contour = Some(pitch);
                        if matches!(
                            deck.analysis_state,
                            AnalysisState::Queued | AnalysisState::NotStarted
                        ) {
                            deck.analysis_state = AnalysisState::Running { progress: 0.5 };
                        }
                    }
                }
            }
            AnalysisEvent::SegmentsReady {
                deck_idx,
                track_id,
                segments,
            } => {
                if let Some(deck) = self.decks.get_mut(deck_idx) {
                    if deck.snap.track_id == track_id {
                        deck.segments = Some(segments);
                        deck.analysis_state = AnalysisState::Done;
                    }
                }
            }
            AnalysisEvent::Failed {
                deck_idx,
                track_id,
                reason,
            } => {
                if let Some(deck) = self.decks.get_mut(deck_idx) {
                    if deck.snap.track_id == track_id {
                        deck.analysis_state = AnalysisState::Failed { reason };
                    }
                }
            }
        }
    }

    pub fn on_air_deck(&self) -> Option<u8> {
        self.decks
            .iter()
            .find(|d| d.role == DeckRole::OnAir)
            .map(|d| d.layer_idx)
    }

    pub fn next_deck(&self) -> Option<u8> {
        self.decks
            .iter()
            .find(|d| d.role == DeckRole::Next)
            .map(|d| d.layer_idx)
    }
}
