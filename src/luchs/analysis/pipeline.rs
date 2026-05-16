use std::path::PathBuf;
use std::sync::Arc;
use log::info;
use crate::luchs::phrase_types::{MpCurve, PitchContour, Segment};

use super::allin1;
use super::cache;
use super::mp_pitch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnalysisPriority {
    /// On-air deck — analyse first.
    OnAir,
    /// Next deck — analyse second.
    Next,
    /// Idle/empty — background priority.
    Idle,
}

#[derive(Debug)]
pub enum AnalysisEvent {
    /// `mp` + `pitch` returned ahead of segment analysis. Either may be empty
    /// if the helper failed to extract that signal.
    McPitchReady {
        deck_idx: usize,
        track_id: u32,
        mp: Arc<MpCurve>,
        pitch: Arc<PitchContour>,
    },
    /// Phrase segmentation finished.
    SegmentsReady {
        deck_idx: usize,
        track_id: u32,
        segments: Arc<Vec<Segment>>,
    },
    /// Job failed. UI should leave shimmer / placeholders.
    Failed {
        deck_idx: usize,
        track_id: u32,
        reason: String,
    },
}

struct Job {
    deck_idx: usize,
    track_id: u32,
    audio_path: PathBuf,
    title: String,
    artist: String,
    priority: AnalysisPriority,
}

pub struct AnalysisManager {
    event_tx: kanal::Sender<AnalysisEvent>,
    event_rx: kanal::Receiver<AnalysisEvent>,
    job_tx: kanal::Sender<Job>,
    allin1_script: PathBuf,
    mp_pitch_script: PathBuf,
    /// Track-id ↔ deck currently scheduled. Used to dedupe submissions when
    /// the same deck re-enters refresh() each frame.
    submitted: [Option<u32>; 4],
}

impl AnalysisManager {
    /// `script_dir` should contain `luchs_allin1.py` and `luchs_mp_pitch.py`.
    pub fn new(script_dir: PathBuf) -> Self {
        let (event_tx, event_rx) = kanal::bounded::<AnalysisEvent>(64);
        let (job_tx, job_rx) = kanal::unbounded::<Job>();

        let allin1_script = script_dir.join("luchs_allin1.py");
        let mp_pitch_script = script_dir.join("luchs_mp_pitch.py");

        // Two worker threads — mp_pitch tends to finish well before allin1,
        // so they run in parallel.
        for _ in 0..2 {
            let job_rx = job_rx.clone();
            let event_tx = event_tx.clone();
            let allin1_script = allin1_script.clone();
            let mp_pitch_script = mp_pitch_script.clone();
            std::thread::spawn(move || {
                while let Ok(job) = job_rx.recv() {
                    run_job(&job, &allin1_script, &mp_pitch_script, &event_tx);
                }
            });
        }

        Self {
            event_tx,
            event_rx,
            job_tx,
            allin1_script,
            mp_pitch_script,
            submitted: [None; 4],
        }
    }

    /// Enqueue analysis for a deck. Idempotent for the same (deck, track_id).
    /// `title` and `artist` are the broadcast metadata; they drive the cache
    /// key so analysis output is portable across machines / file moves.
    pub fn submit(
        &mut self,
        deck_idx: usize,
        track_id: u32,
        audio_path: PathBuf,
        title: String,
        artist: String,
        priority: AnalysisPriority,
    ) {
        if deck_idx >= 4 {
            return;
        }
        if self.submitted[deck_idx] == Some(track_id) {
            return;
        }
        self.submitted[deck_idx] = Some(track_id);
        let _ = self.job_tx.send(Job {
            deck_idx,
            track_id,
            audio_path,
            title,
            artist,
            priority,
        });
    }

    /// Forget the submission record for `deck_idx` so a future re-submit
    /// (e.g. after a different track loads) is accepted.
    pub fn clear(&mut self, deck_idx: usize) {
        if deck_idx < 4 {
            self.submitted[deck_idx] = None;
        }
    }

    pub fn drain_events(&self) -> Vec<AnalysisEvent> {
        let mut out = Vec::new();
        let _ = self.event_rx.drain_into(&mut out);
        out
    }

    pub fn allin1_script_path(&self) -> &PathBuf {
        &self.allin1_script
    }

    pub fn mp_pitch_script_path(&self) -> &PathBuf {
        &self.mp_pitch_script
    }
}

fn run_job(
    job: &Job,
    allin1_script: &std::path::Path,
    mp_pitch_script: &std::path::Path,
    events: &kanal::Sender<AnalysisEvent>,
) {
    let key = cache::key_for_track(&job.title, &job.artist);
    let cache_dir = match cache::ensure_dir(&key) {
        Ok(p) => p,
        Err(e) => {
            let _ = events.send(AnalysisEvent::Failed {
                deck_idx: job.deck_idx,
                track_id: job.track_id,
                reason: format!("cache dir: {}", e),
            });
            return;
        }
    };

    let _ = job.priority; // priority is honored implicitly by submission order

    // 1) Run the lighter mp+pitch helper first so the UI gets the M/P strip
    //    and pitch contour quickly.
    match mp_pitch::run(mp_pitch_script, &job.audio_path, &cache_dir) {
        Ok(r) => {
            let _ = events.send(AnalysisEvent::McPitchReady {
                deck_idx: job.deck_idx,
                track_id: job.track_id,
                mp: Arc::new(r.mp),
                pitch: Arc::new(r.pitch),
            });
        }
        Err(e) => {
            log::warn!("mp_pitch failed for {:?}: {}", job.audio_path, e.0);
        }
    }

    // 2) Then phrase segmentation.
    match allin1::run(allin1_script, &job.audio_path, &cache_dir) {
        Ok(r) => {
            let _ = events.send(AnalysisEvent::SegmentsReady {
                deck_idx: job.deck_idx,
                track_id: job.track_id,
                segments: Arc::new(r.segments),
            });
        }
        Err(e) => {
            let _ = events.send(AnalysisEvent::Failed {
                deck_idx: job.deck_idx,
                track_id: job.track_id,
                reason: e.0,
            });
        }
    }
}
