use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use rosc::{OscMessage, OscPacket, OscType};


#[derive(Debug, Clone)]
pub enum OscEvent {
    /// A phrase boundary was crossed (kind may equal previous when the new
    /// segment is the same kind as the old — spec wants verse→verse to still
    /// fire). Carries the new phrase kind.
    Phrase {
        segment_idx: i32,
    },
    /// A new beat tick was detected on the on-air deck.
    Beat {
        beat_number: u32,
    },
}

/// Snapshot of OSC configuration the sender uses. Cheap to clone (just an
/// `Arc` over the inner state); updated atomically by the UI when the user
/// hits Save in the settings modal.
#[derive(Clone)]
pub struct OscConfig {
    inner: Arc<Mutex<OscConfigInner>>,
}

struct OscConfigInner {
    endpoints: Vec<SocketAddr>,
    phrase_address: String,
    beat_address: String,
    forward_all_decks: bool,
}

impl OscConfig {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(OscConfigInner {
                endpoints: Vec::new(),
                phrase_address: "/luchs/phrase".to_string(),
                beat_address: "/luchs/beat".to_string(),
                forward_all_decks: false,
            })),
        }
    }

    pub fn update(
        &self,
        endpoints: Vec<SocketAddr>,
        phrase_address: String,
        beat_address: String,
        forward_all_decks: bool,
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.endpoints = endpoints;
            inner.phrase_address = phrase_address;
            inner.beat_address = beat_address;
            inner.forward_all_decks = forward_all_decks;
        }
    }

    pub fn snapshot(&self) -> Option<(Vec<SocketAddr>, String, String, bool)> {
        let inner = self.inner.lock().ok()?;
        Some((
            inner.endpoints.clone(),
            inner.phrase_address.clone(),
            inner.beat_address.clone(),
            inner.forward_all_decks,
        ))
    }
}

impl Default for OscConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Send-only OSC dispatcher. Owns a single bound UDP socket and a snapshot
/// of the user-configured endpoint list. Cheap to call from the UI thread:
/// each `send_phrase` / `send_beat` encodes one OSC message and fires N
/// `send_to` calls (typically N = 0..2 endpoints).
pub struct OscSender {
    socket: Option<UdpSocket>,
    config: OscConfig,
}

impl OscSender {
    pub fn new(config: OscConfig) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").ok();
        if let Some(s) = socket.as_ref() {
            let _ = s.set_nonblocking(true);
        }
        Self { socket, config }
    }

    pub fn dispatch(&self, event: OscEvent) {
        let Some(socket) = self.socket.as_ref() else {
            return;
        };
        let Some((endpoints, phrase_addr, beat_addr, _)) = self.config.snapshot() else {
            return;
        };
        if endpoints.is_empty() {
            return;
        }
        let (addr, args) = match event {
            OscEvent::Phrase {
                segment_idx,
            } => (
                phrase_addr,
                vec![
                    OscType::Int(segment_idx),
                ],
            ),
            OscEvent::Beat {
                beat_number,
            } => (
                beat_addr,
                vec![
                    OscType::Int(beat_number as i32),
                ],
            ),
        };
        let msg = OscMessage { addr, args };
        let Ok(buf) = rosc::encoder::encode(&OscPacket::Message(msg)) else {
            return;
        };
        for endpoint in endpoints {
            if let Err(e) = socket.send_to(&buf, endpoint) {
                log::debug!("osc send to {} failed: {}", endpoint, e);
            }
        }
    }
}
