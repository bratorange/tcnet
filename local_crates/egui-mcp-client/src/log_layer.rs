//! MCP Log Layer for capturing application logs
//!
//! This module provides a tracing-subscriber Layer that captures log entries
//! into a buffer for retrieval via MCP tools.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use egui_mcp_client::McpLogLayer;
//! use tracing_subscriber::prelude::*;
//!
//! fn main() {
//!     let (mcp_layer, log_buffer) = McpLogLayer::new(1000);
//!
//!     tracing_subscriber::registry()
//!         .with(mcp_layer)
//!         .with(tracing_subscriber::fmt::layer())
//!         .init();
//!
//!     let mcp_client = McpClient::new().with_log_buffer(log_buffer);
//!     // ... run egui app
//! }
//! ```

use egui_mcp_protocol::LogEntry;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// Shared log buffer type
pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;

/// Default maximum message length (8KB)
pub const DEFAULT_MAX_MESSAGE_LENGTH: usize = 8 * 1024;

/// A tracing Layer that captures log entries for MCP access
pub struct McpLogLayer {
    buffer: LogBuffer,
    max_entries: usize,
    max_message_length: usize,
}

impl McpLogLayer {
    /// Create a new MCP log layer with the specified maximum number of entries
    ///
    /// Uses `DEFAULT_MAX_MESSAGE_LENGTH` (8KB) for message truncation.
    ///
    /// Returns the layer and a shared reference to the log buffer that can be
    /// passed to `McpClient::with_log_buffer()`.
    pub fn new(max_entries: usize) -> (Self, LogBuffer) {
        Self::with_message_limit(max_entries, DEFAULT_MAX_MESSAGE_LENGTH)
    }

    /// Create a new MCP log layer with custom entry and message limits
    ///
    /// - `max_entries`: Maximum number of log entries to keep in the buffer
    /// - `max_message_length`: Maximum length of each log message (truncated if exceeded)
    ///
    /// Returns the layer and a shared reference to the log buffer.
    pub fn with_message_limit(max_entries: usize, max_message_length: usize) -> (Self, LogBuffer) {
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(max_entries)));
        let layer = Self {
            buffer: buffer.clone(),
            max_entries,
            max_message_length,
        };
        (layer, buffer)
    }

    /// Get a reference to the log buffer
    pub fn buffer(&self) -> LogBuffer {
        self.buffer.clone()
    }

    /// Truncate a message if it exceeds the maximum length
    fn truncate_message(&self, message: String) -> String {
        if message.len() <= self.max_message_length {
            message
        } else {
            // Truncate at a char boundary and add ellipsis
            let mut truncated = String::with_capacity(self.max_message_length + 3);
            for (i, c) in message.char_indices() {
                if i + c.len_utf8() > self.max_message_length - 3 {
                    truncated.push_str("...");
                    break;
                }
                truncated.push(c);
            }
            truncated
        }
    }
}

/// Visitor to extract the message field from a tracing event
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
            // Remove surrounding quotes if present
            if self.message.starts_with('"') && self.message.ends_with('"') {
                self.message = self.message[1..self.message.len() - 1].to_string();
            }
        }
    }
}

impl<S> Layer<S> for McpLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        // If no message field, try to get any field content
        if visitor.message.is_empty() {
            let mut any_visitor = AnyFieldVisitor::default();
            event.record(&mut any_visitor);
            visitor.message = any_visitor.fields.join(", ");
        }

        let entry = LogEntry {
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_string(),
            message: self.truncate_message(visitor.message),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };

        let mut buf = self.buffer.lock();
        buf.push_back(entry);
        while buf.len() > self.max_entries {
            buf.pop_front();
        }
    }
}

/// Visitor to capture any field content when no message field is present
#[derive(Default)]
struct AnyFieldVisitor {
    fields: Vec<String>,
}

impl Visit for AnyFieldVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.push(format!("{}={:?}", field.name(), value));
    }
}

/// Helper functions for filtering logs by level
pub fn level_to_priority(level: &str) -> u8 {
    match level.to_uppercase().as_str() {
        "ERROR" => 5,
        "WARN" => 4,
        "INFO" => 3,
        "DEBUG" => 2,
        "TRACE" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_message_short() {
        let (layer, _) = McpLogLayer::with_message_limit(100, 50);
        let msg = "Hello, World!".to_string();
        assert_eq!(layer.truncate_message(msg.clone()), msg);
    }

    #[test]
    fn test_truncate_message_exact_limit() {
        let (layer, _) = McpLogLayer::with_message_limit(100, 13);
        let msg = "Hello, World!".to_string(); // exactly 13 chars
        assert_eq!(layer.truncate_message(msg.clone()), msg);
    }

    #[test]
    fn test_truncate_message_over_limit() {
        let (layer, _) = McpLogLayer::with_message_limit(100, 10);
        let msg = "Hello, World!".to_string();
        let truncated = layer.truncate_message(msg);
        assert!(truncated.ends_with("..."));
        assert!(truncated.len() <= 10);
    }

    #[test]
    fn test_truncate_message_unicode() {
        let (layer, _) = McpLogLayer::with_message_limit(100, 10);
        let msg = "こんにちは世界".to_string(); // 7 chars, 21 bytes
        let truncated = layer.truncate_message(msg);
        // Should truncate at char boundary
        assert!(truncated.ends_with("..."));
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn test_level_to_priority() {
        assert_eq!(level_to_priority("ERROR"), 5);
        assert_eq!(level_to_priority("WARN"), 4);
        assert_eq!(level_to_priority("INFO"), 3);
        assert_eq!(level_to_priority("DEBUG"), 2);
        assert_eq!(level_to_priority("TRACE"), 1);
        assert_eq!(level_to_priority("error"), 5); // case insensitive
        assert_eq!(level_to_priority("unknown"), 0);
    }
}
