//! Typed builder for TCNet Text messages (wire-level msg type 128).
//!
//! Text packets carry an opaque ASCII string — typically a chat or
//! status message between operators on a TCNet network.  This module
//! is the typed companion to [`ControlPath`](super::ControlPath):
//! same step=0/1 layout, but with a less constrained payload.

use crate::protocol::TextData;

/// One TCNet Text payload, ready for embedding in [`TextData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMessage {
    bytes: Vec<u8>,
}

impl TextMessage {
    /// Wrap an ASCII string.
    pub fn new(s: impl Into<String>) -> Self {
        Self {
            bytes: s.into().into_bytes(),
        }
    }

    /// Wrap raw bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// All-ASCII check.
    pub fn is_ascii(&self) -> bool {
        self.bytes.iter().all(|b| (0x20..=0x7E).contains(b))
    }

    /// View as `&str` if all-ASCII; otherwise `None`.
    pub fn as_str(&self) -> Option<&str> {
        if self.is_ascii() {
            // SAFETY: all-ASCII implies valid UTF-8.
            Some(unsafe { std::str::from_utf8_unchecked(&self.bytes) })
        } else {
            None
        }
    }

    /// Raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Build an outgoing initiator [`TextData`] (step = 0).
    pub fn into_initial(self) -> TextData {
        TextData::new_initial(self.bytes)
    }

    /// Build a response [`TextData`] (step = 1).
    pub fn into_response(self) -> TextData {
        TextData::new_response(self.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_wraps_string_and_round_trips() {
        let m = TextMessage::new("hello world");
        assert!(m.is_ascii());
        assert_eq!(m.as_str(), Some("hello world"));
        assert_eq!(m.as_bytes(), b"hello world");
    }

    #[test]
    fn non_ascii_reports_correctly() {
        let m = TextMessage::from_bytes(vec![0xFF, 0xFE]);
        assert!(!m.is_ascii());
        assert!(m.as_str().is_none());
    }

    #[test]
    fn into_initial_emits_step_zero_text_data() {
        let td = TextMessage::new("hello").into_initial();
        assert_eq!(td.step(), 0);
        assert_eq!(td.text_data(), b"hello");
    }

    #[test]
    fn into_response_emits_step_one_text_data() {
        let td = TextMessage::new("ack").into_response();
        assert_eq!(td.step(), 1);
        assert_eq!(td.text_data(), b"ack");
    }
}
