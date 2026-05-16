//! Constants used throughout the MCP server
//!
//! This module centralizes magic numbers for better maintainability.

/// Default alpha value for RGB color parsing (semi-transparent)
pub const DEFAULT_COLOR_ALPHA: u8 = 200;

/// Default timeout for wait operations in milliseconds
pub const DEFAULT_WAIT_TIMEOUT_MS: u64 = 5000;

/// Polling interval for wait operations in milliseconds
pub const WAIT_POLL_INTERVAL_MS: u64 = 100;

/// Default highlight color (red with semi-transparency)
#[allow(dead_code)]
pub const DEFAULT_HIGHLIGHT_COLOR: [u8; 4] = [255, 0, 0, DEFAULT_COLOR_ALPHA];

/// Minimum alpha value for diff visualization
#[allow(dead_code)]
pub const DIFF_MIN_ALPHA: u8 = 50;

/// Alpha scaling factor for diff visualization (0.0-1.0)
#[allow(dead_code)]
pub const DIFF_ALPHA_SCALE: f32 = 0.8;
