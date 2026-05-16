//! MCP tool implementations
//!
//! This module contains the actual implementation logic for MCP tools.
//! The main.rs file contains thin wrappers that delegate to these implementations.

pub mod action;
pub mod basic;
pub mod component;
pub mod highlight;
pub mod input;
pub mod logging;
pub mod perf;
pub mod screenshot;
pub mod selection;
pub mod snapshot;
pub mod state;
pub mod text;
pub mod tree;
pub mod value;
pub mod wait;

use serde_json::json;

/// Common result type for tool implementations
pub type ToolResult = String;

/// Helper to create a success JSON response
pub fn success_response(message: impl Into<String>) -> ToolResult {
    json!({
        "success": true,
        "message": message.into()
    })
    .to_string()
}

/// Helper to create an error JSON response
pub fn error_response(error_type: &str, message: impl Into<String>) -> ToolResult {
    json!({
        "error": error_type,
        "message": message.into()
    })
    .to_string()
}

/// Helper to create a "not connected" error response
pub fn not_connected_error() -> ToolResult {
    error_response(
        "not_connected",
        "No egui application socket found. Make sure the egui app is running with egui-mcp-client.",
    )
}

/// Helper to create a "not available on this platform" error response
#[allow(dead_code)]
pub fn not_available_error(feature: &str) -> ToolResult {
    error_response(
        "not_available",
        format!("{} requires AT-SPI on Linux.", feature),
    )
}

/// Helper to create an AT-SPI connection error response
#[cfg(target_os = "linux")]
pub fn atspi_connection_error(e: impl std::fmt::Display) -> ToolResult {
    error_response(
        "atspi_connection_error",
        format!("Failed to connect to AT-SPI: {}", e),
    )
}

/// Helper to parse element ID from string
pub fn parse_element_id(id: &str) -> Result<u64, ToolResult> {
    id.parse::<u64>()
        .map_err(|_| error_response("invalid_id", format!("Invalid element ID: {}", id)))
}
