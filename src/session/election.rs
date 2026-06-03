//! Master-election state machine.
//!
//! In TCNet, multiple peers may run with `NodeType::Auto` or
//! `NodeType::Master`.  The network must settle on a single master at
//! any one time, because Time-packet emissions are anchored on the
//! master clock.  This module is the tiny FSM that observes peer
//! claims and either picks a winner or surfaces a stalemate for the
//! session task to break.
//!
//! ```text
//!   ┌──────────┐ first master claim    ┌──────────────┐  better candidate
//!   │ Watching │ ─────────────────────►│  Elected     │ ─────────────────┐
//!   └────┬─────┘                       └──────┬───────┘   (re-elect)     │
//!        │                                    │                          │
//!        │ we're Auto + a contender appears   │ master leaves            │
//!        ▼                                    ▼                          │
//!   ┌────────────┐                       ┌──────────┐                    │
//!   │ Contending │                       │ Watching │ ◄──────────────────┘
//!   └────────────┘                       └──────────┘
//! ```
//!
//! Tie-break order (from highest priority):
//!
//! 1. Higher [`ElectionCandidate::uptime_secs`] wins (longer-running
//!    node is more likely to have an accurate clock).
//! 2. Older [`ElectionCandidate::announced_at`] wins (settled earlier).
//! 3. Lower `node_id` wins (deterministic fallback so all peers
//!    converge on the same answer even if uptime / ts tie).

use crate::protocol::NodeId;
use std::cmp::Ordering;
use std::net::SocketAddrV4;
use std::time::Instant;

/// A peer that has announced itself as `Master` (or `Auto` while
/// contending).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectionCandidate {
    pub node_id: NodeId,
    pub addr: SocketAddrV4,
    /// Peer's own uptime in seconds, read from its
    /// [`ManagementHeader`](crate::protocol::ManagementHeader)
    /// `timestamp` / OptIn payload.  Larger = older = preferred.
    pub uptime_secs: u32,
    /// When *we* first heard from this candidate.
    pub announced_at: Instant,
}

impl ElectionCandidate {
    /// Spec tie-break: `(uptime DESC, announced_at ASC, node_id ASC)`.
    /// Used by [`Election::observe`](Election::observe).
    pub fn priority_cmp(&self, other: &Self) -> Ordering {
        // Higher uptime wins → reverse cmp.
        other
            .uptime_secs
            .cmp(&self.uptime_secs)
            .then_with(|| self.announced_at.cmp(&other.announced_at))
            .then_with(|| self.node_id.cmp(&other.node_id))
    }
}

/// Who currently holds the master role (if any).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectionWinner {
    pub node_id: NodeId,
    pub addr: SocketAddrV4,
    /// When we first declared this node the winner.
    pub elected_at: Instant,
}

/// Current state of the local-perspective election machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElectionState {
    /// No master observed; we're not contending.  Default for `Slave`
    /// / `Repeater` local nodes.
    #[default]
    Watching,
    /// We (local) are running as `Auto` and a contender is in play.
    /// `since` is when we entered this state — used to break ties
    /// when multiple Auto nodes converge.
    Contending { since: Instant },
    /// A clear winner has been chosen.
    Elected(ElectionWinner),
}

/// The election state machine. Driven by the dispatcher's election driver.
#[derive(Debug, Clone, Default)]
pub struct Election {
    pub state: ElectionState,
}

impl Election {
    pub fn new() -> Self {
        Self::default()
    }

    /// Re-evaluate the current state given an authoritative
    /// (de-duplicated) candidate set.  Returns the new state.
    ///
    /// `candidates.is_empty()` → `Watching`.
    /// Otherwise the winner is chosen by [`ElectionCandidate::priority_cmp`].
    pub fn observe(&mut self, candidates: &[ElectionCandidate], now: Instant) -> ElectionState {
        if candidates.is_empty() {
            self.state = ElectionState::Watching;
            return self.state;
        }

        let winner = candidates
            .iter()
            .min_by(|a, b| a.priority_cmp(b))
            .copied()
            .expect("candidates non-empty");

        // Already elected the same node — keep elected_at stable so
        // downstream consumers can detect new winners by ts delta.
        if let ElectionState::Elected(existing) = self.state
            && existing.node_id == winner.node_id {
                return self.state;
            }

        self.state = ElectionState::Elected(ElectionWinner {
            node_id: winner.node_id,
            addr: winner.addr,
            elected_at: now,
        });
        self.state
    }

    /// The election round was lost — a contender beat us.  Local Auto
    /// nodes use this to step down.
    pub fn stand_down(&mut self) {
        self.state = ElectionState::Watching;
    }

    /// We just announced ourselves as Auto/Master — start contending.
    pub fn begin_contending(&mut self, now: Instant) {
        self.state = ElectionState::Contending { since: now };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};
    use std::time::Duration;

    fn cand(node_id: NodeId, uptime: u32, announced_at: Instant) -> ElectionCandidate {
        ElectionCandidate {
            node_id,
            addr: SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, node_id as u8), 65023),
            uptime_secs: uptime,
            announced_at,
        }
    }

    #[test]
    fn empty_candidates_returns_watching() {
        let mut e = Election::new();
        let s = e.observe(&[], Instant::now());
        assert_eq!(s, ElectionState::Watching);
    }

    #[test]
    fn single_candidate_becomes_winner() {
        let mut e = Election::new();
        let t = Instant::now();
        let s = e.observe(&[cand(7, 100, t)], t);
        match s {
            ElectionState::Elected(w) => assert_eq!(w.node_id, 7),
            other => panic!("expected Elected, got {:?}", other),
        }
    }

    #[test]
    fn tie_break_prefers_higher_uptime() {
        let mut e = Election::new();
        let t = Instant::now();
        let s = e.observe(
            &[
                cand(1, 50, t),
                cand(2, 300, t),
                cand(3, 200, t),
            ],
            t,
        );
        let ElectionState::Elected(w) = s else { panic!() };
        assert_eq!(w.node_id, 2, "300 s uptime wins");
    }

    #[test]
    fn tie_break_then_oldest_announced_at() {
        let mut e = Election::new();
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(5);
        let s = e.observe(
            &[
                cand(1, 100, t1),
                cand(2, 100, t0), // earlier announce → wins
            ],
            t1,
        );
        let ElectionState::Elected(w) = s else { panic!() };
        assert_eq!(w.node_id, 2);
    }

    #[test]
    fn tie_break_then_lowest_node_id() {
        let mut e = Election::new();
        let t = Instant::now();
        let s = e.observe(&[cand(5, 100, t), cand(3, 100, t), cand(8, 100, t)], t);
        let ElectionState::Elected(w) = s else { panic!() };
        assert_eq!(w.node_id, 3);
    }

    #[test]
    fn same_winner_does_not_bump_elected_at() {
        let mut e = Election::new();
        let t0 = Instant::now();
        e.observe(&[cand(7, 100, t0)], t0);
        let ElectionState::Elected(w0) = e.state else { panic!() };
        let elected_at_initial = w0.elected_at;

        // Re-observe with the same winner at a later time.
        let t1 = t0 + Duration::from_secs(3);
        e.observe(&[cand(7, 103, t0)], t1);
        let ElectionState::Elected(w1) = e.state else { panic!() };
        assert_eq!(w1.elected_at, elected_at_initial, "elected_at unchanged");
    }

    #[test]
    fn new_winner_resets_elected_at() {
        let mut e = Election::new();
        let t0 = Instant::now();
        e.observe(&[cand(7, 100, t0)], t0);

        let t1 = t0 + Duration::from_secs(3);
        e.observe(&[cand(9, 999, t0)], t1);
        let ElectionState::Elected(w) = e.state else { panic!() };
        assert_eq!(w.node_id, 9);
        assert_eq!(w.elected_at, t1);
    }

    #[test]
    fn stand_down_returns_to_watching() {
        let mut e = Election::new();
        let t = Instant::now();
        e.observe(&[cand(7, 100, t)], t);
        e.stand_down();
        assert_eq!(e.state, ElectionState::Watching);
    }

    #[test]
    fn begin_contending_records_timestamp() {
        let mut e = Election::new();
        let t = Instant::now();
        e.begin_contending(t);
        assert_eq!(e.state, ElectionState::Contending { since: t });
    }
}
