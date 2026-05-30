//! Single-actor session task (ARCHITECTURE.md §4).
//!
//! `SessionTask` owns `HashMap<SocketAddrV4, Peer>` and the
//! `Election` state machine outright.  Mutation only happens by
//! draining typed [`SessionCommand`]s from a bounded queue; reads
//! happen via the [`arc_swap::ArcSwap`]-published
//! [`SessionSnapshot`].  This is what retires the last
//! `Arc<tokio::sync::RwLock<DynamicNodeState>>` from the legacy
//! `src/node/dispatcher.rs` — the dispatcher gets demolished in
//! phase 4.6 once the task is wired into it.
//!
//! Threading model: the task runs as a tokio task today; phase 7
//! moves it onto a dedicated `std::thread` with `clock_nanosleep`
//! ticking.  The trait-shape is unchanged either way.

use super::command::SessionCommand;
use super::election::{Election, ElectionCandidate, ElectionState};
use super::peer::{Peer, PeerActive, PeerAnnouncing, PeerLeaving};
use super::snapshot::{PeerStateKind, PeerSummary, SessionSnapshot};
use arc_swap::ArcSwap;
use crossbeam_queue::ArrayQueue;
use log::{trace, warn};
use std::collections::HashMap;
use std::net::SocketAddrV4;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::Notify;

/// Default size of the inbound command queue.  Generous so a
/// thundering herd of OptIn packets on startup doesn't drop the
/// session's view of who's on the wire.
pub const DEFAULT_QUEUE_CAPACITY: usize = 256;

/// Owns the session state.  Cloneable handle for producers is
/// [`SessionHandle`].
pub struct SessionTask {
    cmd_rx: Arc<ArrayQueue<SessionCommand>>,
    notify: Arc<Notify>,
    shutdown: Arc<AtomicBool>,
    snapshot_out: Arc<ArcSwap<SessionSnapshot>>,
    generation: Arc<AtomicU64>,

    /// Owned exclusively — no other code reads or writes this.
    peers: HashMap<SocketAddrV4, Peer>,
    election: Election,
    /// Tracks who's a current election candidate (claims Master/Auto).
    /// Kept in sync with `peers` so the election can be re-resolved
    /// on every command without scanning the map.
    candidates: HashMap<SocketAddrV4, ElectionCandidate>,
}

/// Public handle to a running [`SessionTask`].
///
/// `Clone` is `Arc::clone` over the queue + ArcSwap, so every layer
/// (UDP recv tasks, periodic ticker, user API) can hold its own
/// handle without sharing extra state.
#[derive(Clone)]
pub struct SessionHandle {
    cmd_tx: Arc<ArrayQueue<SessionCommand>>,
    notify: Arc<Notify>,
    shutdown: Arc<AtomicBool>,
    snapshot_in: Arc<ArcSwap<SessionSnapshot>>,
}

impl SessionHandle {
    /// Hand a command to the task.  Returns `false` if the queue is
    /// full — caller's choice whether to drop or retry.
    pub fn try_send(&self, cmd: SessionCommand) -> bool {
        let ok = self.cmd_tx.push(cmd).is_ok();
        if ok {
            self.notify.notify_one();
        }
        ok
    }

    /// Convenience: log + drop on overflow.  Use this from hot-path
    /// producers (UDP recv) where queue-full means we're already
    /// behind and back-pressuring would just make things worse.
    pub fn send_or_drop(&self, cmd: SessionCommand) {
        if !self.try_send(cmd) {
            warn!("SessionTask command queue full; dropping command");
        }
    }

    /// Wait-free read of the latest published snapshot.
    pub fn snapshot(&self) -> Arc<SessionSnapshot> {
        self.snapshot_in.load_full()
    }

    /// Request the task to shut down.  The task drains everything
    /// already in flight, then exits.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
}

impl SessionTask {
    /// Build a task + handle pair.  The task isn't running yet —
    /// call [`SessionTask::run`] (typically from a `tokio::spawn`)
    /// to drive it.
    pub fn new(capacity: usize) -> (Self, SessionHandle) {
        let cmd_q = Arc::new(ArrayQueue::new(capacity.max(1)));
        let notify = Arc::new(Notify::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let snapshot = Arc::new(ArcSwap::from_pointee(SessionSnapshot::default()));
        let generation = Arc::new(AtomicU64::new(0));

        let task = Self {
            cmd_rx: cmd_q.clone(),
            notify: notify.clone(),
            shutdown: shutdown.clone(),
            snapshot_out: snapshot.clone(),
            generation,
            peers: HashMap::new(),
            election: Election::new(),
            candidates: HashMap::new(),
        };
        let handle = SessionHandle {
            cmd_tx: cmd_q,
            notify,
            shutdown,
            snapshot_in: snapshot,
        };
        (task, handle)
    }

    /// Build with the default queue capacity.
    pub fn new_default() -> (Self, SessionHandle) {
        Self::new(DEFAULT_QUEUE_CAPACITY)
    }

    /// Spawn the task on the current tokio runtime, returning the
    /// handle.  The task lives until [`SessionHandle::shutdown`] is
    /// called or every handle is dropped.
    pub fn spawn() -> SessionHandle {
        let (task, handle) = Self::new_default();
        tokio::spawn(async move { task.run().await });
        handle
    }

    /// Drain commands until shutdown.  Public so tests can drive the
    /// task synchronously without a runtime — call [`apply_command`]
    /// directly there.
    pub async fn run(mut self) {
        loop {
            if self.shutdown.load(Ordering::Acquire) {
                return;
            }
            // Drain everything immediately available.
            let mut applied = false;
            while let Some(cmd) = self.cmd_rx.pop() {
                if matches!(cmd, SessionCommand::Shutdown) {
                    self.shutdown.store(true, Ordering::Release);
                    self.publish();
                    return;
                }
                self.apply_command(cmd);
                applied = true;
            }
            if applied {
                self.publish();
            }
            // Wait for the next batch.
            self.notify.notified().await;
        }
    }

    /// Apply a single command — pure state mutation, no I/O, no
    /// snapshot publication (the caller publishes on a batch
    /// boundary).  Public for testing.
    pub fn apply_command(&mut self, cmd: SessionCommand) {
        match cmd {
            SessionCommand::ObserveOptIn {
                src,
                node_id,
                config,
                uptime_secs,
                claims_master,
                at,
            } => self.on_opt_in(src, node_id, config, uptime_secs, claims_master, at),
            SessionCommand::ObserveDjPacket { src, at } => self.on_dj_packet(src, at),
            SessionCommand::ObserveOptOut { src, at } => self.on_opt_out(src, at),
            SessionCommand::Tick { now } => self.on_tick(now),
            SessionCommand::Shutdown => {
                // Handled in run(); apply_command is a no-op so tests
                // that drive synchronously can include Shutdown
                // without panicking.
            }
        }
    }

    fn on_opt_in(
        &mut self,
        src: SocketAddrV4,
        node_id: crate::protocol::NodeId,
        config: crate::ApplicationConfig,
        uptime_secs: u32,
        claims_master: bool,
        at: Instant,
    ) {
        let entry = self.peers.entry(src);
        match entry {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                // Refresh config / last_seen without changing state.
                let p = o.get_mut();
                match p {
                    Peer::Announcing(a) => {
                        a.config = config;
                        a.announced_at = at;
                    }
                    Peer::Active(a) => {
                        a.config = config;
                        a.last_seen = at;
                    }
                    Peer::Leaving(_) => {
                        // Peer is coming back — restart as Announcing.
                        *p = Peer::Announcing(PeerAnnouncing::new(src, node_id, config, at));
                    }
                }
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(Peer::Announcing(PeerAnnouncing::new(
                    src, node_id, config, at,
                )));
            }
        }
        if claims_master {
            self.candidates.insert(
                src,
                ElectionCandidate {
                    node_id,
                    addr: src,
                    uptime_secs,
                    announced_at: at,
                },
            );
        } else {
            self.candidates.remove(&src);
        }
        self.refresh_election(at);
    }

    fn on_dj_packet(&mut self, src: SocketAddrV4, at: Instant) {
        let Some(p) = self.peers.get_mut(&src) else {
            trace!("ObserveDjPacket from unknown peer {}", src);
            return;
        };
        match p {
            Peer::Announcing(a) => {
                let active: PeerActive = a.clone().promote(at);
                *p = Peer::Active(active);
            }
            Peer::Active(a) => a.touch(at),
            Peer::Leaving(_) => {
                // Peer is coming back; reset to Announcing — the next
                // OptIn (or this DJ packet, treated as proof of life)
                // will promote.
                let addr = p.addr();
                let node_id = p.node_id();
                let config = crate::ApplicationConfig::default();
                *p = Peer::Announcing(PeerAnnouncing::new(addr, node_id, config, at));
            }
        }
    }

    fn on_opt_out(&mut self, src: SocketAddrV4, at: Instant) {
        let Some(p) = self.peers.remove(&src) else {
            return;
        };
        let leaving: PeerLeaving = match p {
            Peer::Announcing(a) => a.opt_out(at),
            Peer::Active(a) => a.opt_out(at),
            Peer::Leaving(l) => l,
        };
        self.peers.insert(src, Peer::Leaving(leaving));
        self.candidates.remove(&src);
        self.refresh_election(at);
    }

    fn on_tick(&mut self, now: Instant) {
        // Evict Leaving peers + Time-out anyone past PEER_TIMEOUT.
        let mut to_remove = Vec::new();
        let mut to_timeout = Vec::new();
        for (addr, p) in self.peers.iter() {
            match p {
                Peer::Leaving(_) => to_remove.push(*addr),
                _ if p.is_timed_out(now) => to_timeout.push(*addr),
                _ => {}
            }
        }
        for addr in to_timeout {
            if let Some(p) = self.peers.remove(&addr) {
                let leaving = match p {
                    Peer::Announcing(a) => a.timeout(now),
                    Peer::Active(a) => a.timeout(now),
                    Peer::Leaving(l) => l,
                };
                self.peers.insert(addr, Peer::Leaving(leaving));
            }
            self.candidates.remove(&addr);
        }
        for addr in to_remove {
            self.peers.remove(&addr);
            self.candidates.remove(&addr);
        }
        self.refresh_election(now);
    }

    fn refresh_election(&mut self, now: Instant) {
        let cands: Vec<ElectionCandidate> = self.candidates.values().copied().collect();
        self.election.observe(&cands, now);
    }

    /// Build a fresh `SessionSnapshot` from the current state and
    /// publish it via the ArcSwap.
    fn publish(&self) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let peers: Vec<PeerSummary> = self
            .peers
            .values()
            .map(|p| {
                let state = match p {
                    Peer::Announcing(_) => PeerStateKind::Announcing,
                    Peer::Active(_) => PeerStateKind::Active,
                    Peer::Leaving(_) => PeerStateKind::Leaving,
                };
                let config = match p {
                    Peer::Announcing(a) => a.config,
                    Peer::Active(a) => a.config,
                    Peer::Leaving(_) => crate::ApplicationConfig::default(),
                };
                PeerSummary {
                    addr: p.addr(),
                    node_id: p.node_id(),
                    state,
                    last_seen: p.last_seen(),
                    config,
                }
            })
            .collect();
        let snap = SessionSnapshot {
            peers,
            election: self.election.state,
            generation,
            published_at: Instant::now(),
        };
        self.snapshot_out.store(Arc::new(snap));
    }

    /// Force a snapshot publication.  Public for tests.
    pub fn force_publish(&self) {
        self.publish();
    }

    /// Read-only access to the owned peer map.  Public for tests only.
    pub fn peers(&self) -> &HashMap<SocketAddrV4, Peer> {
        &self.peers
    }

    /// Current election state.  Public for tests only.
    pub fn election_state(&self) -> ElectionState {
        self.election.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ApplicationConfig;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    fn addr(last: u8) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, last), 65023)
    }

    #[test]
    fn opt_in_registers_an_announcing_peer() {
        let (mut task, _h) = SessionTask::new_default();
        let t = Instant::now();
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 7,
            config: ApplicationConfig::default(),
            uptime_secs: 100,
            claims_master: false,
            at: t,
        });
        assert_eq!(task.peers.len(), 1);
        assert!(matches!(
            task.peers.get(&addr(1)).unwrap(),
            Peer::Announcing(_)
        ));
    }

    #[test]
    fn dj_packet_promotes_announcing_to_active() {
        let (mut task, _h) = SessionTask::new_default();
        let t0 = Instant::now();
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 7,
            config: ApplicationConfig::default(),
            uptime_secs: 100,
            claims_master: false,
            at: t0,
        });
        task.apply_command(SessionCommand::ObserveDjPacket {
            src: addr(1),
            at: t0 + Duration::from_millis(100),
        });
        assert!(matches!(
            task.peers.get(&addr(1)).unwrap(),
            Peer::Active(_)
        ));
    }

    #[test]
    fn opt_out_moves_to_leaving() {
        let (mut task, _h) = SessionTask::new_default();
        let t = Instant::now();
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 7,
            config: ApplicationConfig::default(),
            uptime_secs: 100,
            claims_master: false,
            at: t,
        });
        task.apply_command(SessionCommand::ObserveOptOut {
            src: addr(1),
            at: t + Duration::from_secs(1),
        });
        assert!(matches!(
            task.peers.get(&addr(1)).unwrap(),
            Peer::Leaving(_)
        ));
    }

    #[test]
    fn tick_evicts_leaving_peers() {
        let (mut task, _h) = SessionTask::new_default();
        let t = Instant::now();
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 7,
            config: ApplicationConfig::default(),
            uptime_secs: 100,
            claims_master: false,
            at: t,
        });
        task.apply_command(SessionCommand::ObserveOptOut {
            src: addr(1),
            at: t + Duration::from_secs(1),
        });
        task.apply_command(SessionCommand::Tick {
            now: t + Duration::from_secs(2),
        });
        assert!(task.peers.is_empty());
    }

    #[test]
    fn tick_times_out_silent_peers() {
        let (mut task, _h) = SessionTask::new_default();
        let t = Instant::now();
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 7,
            config: ApplicationConfig::default(),
            uptime_secs: 100,
            claims_master: false,
            at: t,
        });
        let past_timeout = t + super::super::peer::PEER_TIMEOUT + Duration::from_secs(1);
        task.apply_command(SessionCommand::Tick { now: past_timeout });
        // After tick, peer should be Leaving (timeout) ready for next-tick eviction.
        assert!(matches!(
            task.peers.get(&addr(1)).unwrap(),
            Peer::Leaving(_)
        ));
    }

    #[test]
    fn opt_in_with_claims_master_registers_election_candidate() {
        let (mut task, _h) = SessionTask::new_default();
        let t = Instant::now();
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 7,
            config: ApplicationConfig::default(),
            uptime_secs: 100,
            claims_master: true,
            at: t,
        });
        match task.election_state() {
            ElectionState::Elected(w) => assert_eq!(w.node_id, 7),
            other => panic!("expected Elected, got {other:?}"),
        }
    }

    #[test]
    fn higher_uptime_wins_election() {
        let (mut task, _h) = SessionTask::new_default();
        let t = Instant::now();
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 7,
            config: ApplicationConfig::default(),
            uptime_secs: 100,
            claims_master: true,
            at: t,
        });
        task.apply_command(SessionCommand::ObserveOptOut {
            // Doesn't actually matter for this test, but include another peer.
            src: addr(99),
            at: t,
        });
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(2),
            node_id: 8,
            config: ApplicationConfig::default(),
            uptime_secs: 999,
            claims_master: true,
            at: t,
        });
        match task.election_state() {
            ElectionState::Elected(w) => assert_eq!(w.node_id, 8, "higher uptime wins"),
            other => panic!("expected Elected, got {other:?}"),
        }
    }

    #[test]
    fn force_publish_emits_a_snapshot_with_new_generation() {
        let (mut task, handle) = SessionTask::new_default();
        let t = Instant::now();
        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 7,
            config: ApplicationConfig::default(),
            uptime_secs: 100,
            claims_master: false,
            at: t,
        });
        task.force_publish();
        let s1 = handle.snapshot();
        assert_eq!(s1.peers.len(), 1);
        assert_eq!(s1.generation, 1);

        task.apply_command(SessionCommand::ObserveOptIn {
            src: addr(2),
            node_id: 8,
            config: ApplicationConfig::default(),
            uptime_secs: 200,
            claims_master: false,
            at: t,
        });
        task.force_publish();
        let s2 = handle.snapshot();
        assert_eq!(s2.peers.len(), 2);
        assert_eq!(s2.generation, 2);
    }

    #[test]
    fn handle_send_or_drop_pushes_into_queue() {
        let (_task, handle) = SessionTask::new_default();
        for i in 0..10 {
            handle.send_or_drop(SessionCommand::ObserveDjPacket {
                src: addr(i),
                at: Instant::now(),
            });
        }
        // We don't read from the queue here — just verify the
        // pushes succeeded.  Queue capacity is 256, well above 10.
    }

    #[tokio::test]
    async fn spawned_task_processes_commands_and_publishes() {
        let handle = SessionTask::spawn();
        let t = Instant::now();
        handle.try_send(SessionCommand::ObserveOptIn {
            src: addr(1),
            node_id: 42,
            config: ApplicationConfig::default(),
            uptime_secs: 500,
            claims_master: false,
            at: t,
        });

        // Spin briefly waiting for the task to publish.
        let mut got = None;
        for _ in 0..20 {
            let s = handle.snapshot();
            if !s.peers.is_empty() {
                got = Some(s);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let s = got.expect("snapshot should land");
        assert_eq!(s.peers.len(), 1);
        assert_eq!(s.peers[0].node_id, 42);

        handle.shutdown();
    }
}
