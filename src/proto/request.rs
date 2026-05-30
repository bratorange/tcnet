//! Typed request/response machinery (ARCHITECTURE.md §5).
//!
//! A TCNet request (msg type 20) asks a peer for some data type
//! (BeatGrid, BigWaveform, ArtworkFile, Cue, …); the peer responds
//! either with the requested payload or with an ErrorNotification
//! (msg type 13).  This module defines the typed [`Pending<T>`]
//! future a caller awaits, plus the [`RequestError`] enum every
//! caller can match on.
//!
//! ## Design
//!
//! ```text
//!  Caller                    Session task                  Wire
//!  ──────                    ────────────                  ────
//!    │  request(addr, kind)
//!    │ ────────────────────►                                  │
//!    │    Pending<T>          allocate slot,                  │
//!    │ ◄────────────────────  send Request →    ────────────► │
//!    │  .await                                                │
//!    │                                          ◄──────────── │ Response
//!    │                        slot.resolve(t)                  │
//!    │ ◄────────────────────                                  │
//!    │  Ok(t)
//! ```
//!
//! The caller owns a `Pending<T>` (a thin wrapper over a
//! `oneshot::Receiver`); the session task owns the matching
//! `PendingSlot<T>` (the `Sender` half) and resolves it on response
//! arrival, ErrorNotification, peer-gone, or timeout.

use std::time::Duration;
use tokio::sync::oneshot;

/// Default request timeout.  Spec page 9 suggests 5 s; the existing
/// luchs code uses the same value.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Why a request failed.
///
/// `non_exhaustive` so phase 9 can add new error sources (e.g.
/// authentication-required-but-missing) without a SemVer bump.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError {
    /// Peer replied with ErrorNotification code `001` (Unknown).
    Unknown,
    /// Peer replied with ErrorNotification code `013` (NotPossible)
    /// — the request is well-formed but the peer can't satisfy it
    /// right now (e.g. layer not playing, no track loaded).
    NotPossible,
    /// Peer replied with ErrorNotification code `014` (Empty) — the
    /// payload is well-formed but contains no data (e.g. an empty
    /// cue table).
    Empty,
    /// No response within [`DEFAULT_REQUEST_TIMEOUT`] (or the per-
    /// request override).
    Timeout,
    /// Peer left the network mid-request (OptOut or silence-timeout).
    PeerGone,
    /// Peer replied with an unrecognised ErrorNotification code.
    Other { code: u16 },
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => f.write_str("peer reported Unknown (001)"),
            Self::NotPossible => f.write_str("peer reported NotPossible (013)"),
            Self::Empty => f.write_str("peer reported Empty (014)"),
            Self::Timeout => f.write_str("request timed out"),
            Self::PeerGone => f.write_str("peer left the network"),
            Self::Other { code } => write!(f, "peer error code {}", code),
        }
    }
}

impl std::error::Error for RequestError {}

impl RequestError {
    /// Translate an on-the-wire ErrorNotification `code` byte into a
    /// typed variant.  Codes per spec page 9.
    pub fn from_code(code: u16) -> Self {
        match code {
            1 => Self::Unknown,
            13 => Self::NotPossible,
            14 => Self::Empty,
            other => Self::Other { code: other },
        }
    }
}

/// The caller's half of an outstanding request — `.await` it to get
/// the response (or an error).
///
/// `Drop`ping a `Pending<T>` cancels the await on the caller side; the
/// session task may still receive the response and discard it.
pub struct Pending<T> {
    rx: oneshot::Receiver<Result<T, RequestError>>,
}

/// The session task's half of an outstanding request — call
/// [`PendingSlot::resolve`] or [`PendingSlot::reject`] to wake the
/// caller's `.await`.
pub struct PendingSlot<T> {
    tx: oneshot::Sender<Result<T, RequestError>>,
}

impl<T> Pending<T> {
    /// Wait for the response.  Returns:
    /// * `Ok(value)` — peer sent the expected payload.
    /// * `Err(RequestError::PeerGone)` — the matching `PendingSlot`
    ///   was dropped without resolving (typically because the peer
    ///   disappeared and the session task swept all of its in-flight
    ///   requests).
    /// * `Err(other)` — the session task called
    ///   [`PendingSlot::reject`] with a specific error.
    pub async fn await_response(self) -> Result<T, RequestError> {
        match self.rx.await {
            Ok(r) => r,
            Err(_canceled) => Err(RequestError::PeerGone),
        }
    }
}

impl<T> PendingSlot<T> {
    /// Wake the caller's `.await` with the response payload.
    pub fn resolve(self, value: T) {
        let _ = self.tx.send(Ok(value));
    }

    /// Wake the caller's `.await` with a specific error.
    pub fn reject(self, err: RequestError) {
        let _ = self.tx.send(Err(err));
    }
}

/// Construct a fresh `(Pending<T>, PendingSlot<T>)` pair.  The caller
/// keeps the [`Pending`]; the session task keeps the [`PendingSlot`]
/// in its in-flight table.
pub fn pending<T>() -> (Pending<T>, PendingSlot<T>) {
    let (tx, rx) = oneshot::channel();
    (Pending { rx }, PendingSlot { tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_delivers_value_to_await() {
        let (p, slot) = pending::<&'static str>();
        slot.resolve("hello");
        let r = p.await_response().await;
        assert_eq!(r, Ok("hello"));
    }

    #[tokio::test]
    async fn reject_delivers_error_to_await() {
        let (p, slot) = pending::<u32>();
        slot.reject(RequestError::Empty);
        let r = p.await_response().await;
        assert_eq!(r, Err(RequestError::Empty));
    }

    #[tokio::test]
    async fn dropped_slot_resolves_as_peer_gone() {
        let (p, slot) = pending::<u32>();
        drop(slot);
        let r = p.await_response().await;
        assert_eq!(r, Err(RequestError::PeerGone));
    }

    #[test]
    fn from_code_maps_well_known_codes() {
        assert_eq!(RequestError::from_code(1), RequestError::Unknown);
        assert_eq!(RequestError::from_code(13), RequestError::NotPossible);
        assert_eq!(RequestError::from_code(14), RequestError::Empty);
        assert_eq!(
            RequestError::from_code(42),
            RequestError::Other { code: 42 }
        );
    }

    #[test]
    fn display_includes_human_text() {
        let e = RequestError::Empty;
        let s = format!("{e}");
        assert!(s.to_lowercase().contains("empty"));
    }
}
