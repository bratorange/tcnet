//! Typed builder for TCNet Keyboard input (wire-level msg type 132).
//!
//! Keyboard packets carry a 2-byte HEX-ASCII scan code — a passthrough
//! for remote-keyboard control of TCNet nodes that subscribe to it.
//!
//! Example: pressing `'A'` (scan code 0x1E) is sent as the HEX-ASCII
//! bytes `[b'1', b'E']`.  This module exposes a typed [`KeyPress`]
//! that hides the HEX-ASCII encoding so callers don't have to do
//! `format!("{:02X}", code)` everywhere.

use crate::protocol::KeyboardData;

/// One keyboard scan-code event.
///
/// `code` is the raw scan code; encoding to HEX-ASCII happens on
/// `into_keyboard_data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub code: u8,
}

impl KeyPress {
    pub fn new(code: u8) -> Self {
        Self { code }
    }

    /// Encode the scan code as the two HEX-ASCII bytes the wire
    /// expects.
    pub fn as_hex_ascii(self) -> [u8; 2] {
        let hi = self.code >> 4;
        let lo = self.code & 0x0F;
        [hex_digit(hi), hex_digit(lo)]
    }

    /// Build the wire-side `KeyboardData`.
    pub fn into_keyboard_data(self) -> KeyboardData {
        KeyboardData::new(self.as_hex_ascii())
    }

    /// Decode a `KeyboardData` back into a `KeyPress`.  Returns
    /// `None` if the 2-byte payload isn't valid HEX-ASCII.
    pub fn from_keyboard_data(kd: &KeyboardData) -> Option<Self> {
        let [hi, lo] = kd.scan_code();
        let hi = hex_value(hi)?;
        let lo = hex_value(lo)?;
        Some(Self {
            code: (hi << 4) | lo,
        })
    }
}

const fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        10..=15 => b'A' + (nibble - 10),
        _ => b'?',
    }
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_code_round_trips_through_hex_ascii() {
        for code in [0x00u8, 0x0A, 0x1E, 0x7F, 0xAB, 0xFF] {
            let kp = KeyPress::new(code);
            let kd = kp.into_keyboard_data();
            let decoded = KeyPress::from_keyboard_data(&kd).expect("valid hex");
            assert_eq!(decoded.code, code, "round-trip failed for 0x{code:02X}");
        }
    }

    #[test]
    fn hex_ascii_uses_uppercase_letters() {
        let kp = KeyPress::new(0xAB);
        assert_eq!(kp.as_hex_ascii(), [b'A', b'B']);
    }

    #[test]
    fn hex_ascii_pads_low_byte_with_zero() {
        let kp = KeyPress::new(0x05);
        assert_eq!(kp.as_hex_ascii(), [b'0', b'5']);
    }

    #[test]
    fn from_keyboard_data_accepts_lowercase_hex() {
        let kd = KeyboardData::new([b'a', b'b']);
        let kp = KeyPress::from_keyboard_data(&kd).expect("lowercase ok");
        assert_eq!(kp.code, 0xAB);
    }

    #[test]
    fn from_keyboard_data_rejects_non_hex() {
        let kd = KeyboardData::new([b'X', b'1']);
        assert!(KeyPress::from_keyboard_data(&kd).is_none());
    }
}
