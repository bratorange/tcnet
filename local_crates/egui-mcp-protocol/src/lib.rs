//! Common protocol definitions for egui-mcp
//!
//! This crate defines the shared types and protocols used for IPC communication
//! between the MCP server and egui client applications.
//!
//! Note: UI tree access, element search, and click/text input operations are
//! handled via AT-SPI on Linux. This protocol is only used for features that
//! require direct client integration (screenshots, coordinate-based input, etc.).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Default socket path for IPC communication.
///
/// Honors `EGUI_MCP_SOCKET` env var (full path) when set, so multiple
/// embedding apps can run side-by-side on distinct sockets. Falls back
/// to `$XDG_RUNTIME_DIR/egui-mcp.sock` (or temp dir on macOS).
pub fn default_socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("EGUI_MCP_SOCKET") {
        return PathBuf::from(p);
    }
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    runtime_dir.join("egui-mcp.sock")
}

/// Information about a UI node (used for AT-SPI responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique identifier for the node
    pub id: u64,
    /// Role of the node (e.g., "Button", "TextInput", "Window")
    pub role: String,
    /// Human-readable label
    pub label: Option<String>,
    /// Current value (for inputs, sliders, etc.)
    pub value: Option<String>,
    /// Bounding rectangle
    pub bounds: Option<Rect>,
    /// Child node IDs
    pub children: Vec<u64>,
    /// Whether the node is toggled (for checkboxes, toggles)
    pub toggled: Option<bool>,
    /// Whether the node is disabled
    pub disabled: bool,
    /// Whether the node has focus
    pub focused: bool,
}

/// A rectangle in screen coordinates
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// UI tree containing all nodes (used for AT-SPI responses)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiTree {
    /// Root node IDs
    pub roots: Vec<u64>,
    /// All nodes in the tree
    pub nodes: Vec<NodeInfo>,
}

/// Mouse button for click operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Log entry captured from the application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level (TRACE, DEBUG, INFO, WARN, ERROR)
    pub level: String,
    /// Target module/crate
    pub target: String,
    /// Log message
    pub message: String,
    /// Timestamp in milliseconds since UNIX epoch
    pub timestamp_ms: u64,
}

/// Frame statistics for performance monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameStats {
    /// Current frames per second
    pub fps: f32,
    /// Average frame time in milliseconds
    pub frame_time_ms: f32,
    /// Minimum frame time in milliseconds
    pub frame_time_min_ms: f32,
    /// Maximum frame time in milliseconds
    pub frame_time_max_ms: f32,
    /// Number of frames sampled
    pub sample_count: usize,
}

/// Performance report from a recording session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfReport {
    /// Recording duration in milliseconds
    pub duration_ms: u64,
    /// Total frames recorded
    pub total_frames: usize,
    /// Average FPS over the recording
    pub avg_fps: f32,
    /// Average frame time in milliseconds
    pub avg_frame_time_ms: f32,
    /// Minimum frame time in milliseconds
    pub min_frame_time_ms: f32,
    /// Maximum frame time in milliseconds
    pub max_frame_time_ms: f32,
    /// 95th percentile frame time in milliseconds
    pub p95_frame_time_ms: f32,
    /// 99th percentile frame time in milliseconds
    pub p99_frame_time_ms: f32,
}

/// Request types for IPC communication
///
/// These are operations that require direct client integration and cannot be
/// performed via AT-SPI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    /// Ping the client to check connection
    Ping,

    /// Request a screenshot of the application window
    TakeScreenshot,

    /// Request a screenshot of a specific region of the application window
    TakeScreenshotRegion {
        /// X coordinate of the region (relative to window)
        x: f32,
        /// Y coordinate of the region (relative to window)
        y: f32,
        /// Width of the region
        width: f32,
        /// Height of the region
        height: f32,
    },

    /// Click at specific screen coordinates
    ClickAt {
        /// X coordinate (relative to window)
        x: f32,
        /// Y coordinate (relative to window)
        y: f32,
        /// Mouse button to click
        button: MouseButton,
    },

    /// Send keyboard input
    KeyboardInput {
        /// Key to press (e.g., "Enter", "Tab", "a", "Ctrl+C")
        key: String,
    },

    /// Scroll at specific coordinates
    Scroll {
        /// X coordinate (relative to window)
        x: f32,
        /// Y coordinate (relative to window)
        y: f32,
        /// Horizontal scroll delta
        delta_x: f32,
        /// Vertical scroll delta
        delta_y: f32,
    },

    /// Move mouse to specific coordinates (for hover effects)
    MoveMouse {
        /// X coordinate (relative to window)
        x: f32,
        /// Y coordinate (relative to window)
        y: f32,
    },

    /// Drag from one position to another
    Drag {
        /// Start X coordinate
        start_x: f32,
        /// Start Y coordinate
        start_y: f32,
        /// End X coordinate
        end_x: f32,
        /// End Y coordinate
        end_y: f32,
        /// Mouse button to use
        button: MouseButton,
    },

    /// Double click at specific screen coordinates
    DoubleClick {
        /// X coordinate (relative to window)
        x: f32,
        /// Y coordinate (relative to window)
        y: f32,
        /// Mouse button to click
        button: MouseButton,
    },

    /// Highlight an element with a colored border
    HighlightElement {
        /// Bounding box x coordinate
        x: f32,
        /// Bounding box y coordinate
        y: f32,
        /// Bounding box width
        width: f32,
        /// Bounding box height
        height: f32,
        /// Color as RGBA (0-255 each)
        color: [u8; 4],
        /// Duration in milliseconds (0 = until cleared)
        duration_ms: u64,
    },

    /// Clear all highlights
    ClearHighlights,

    /// Get recent log entries
    GetLogs {
        /// Minimum log level to return (TRACE, DEBUG, INFO, WARN, ERROR)
        /// If None, returns all levels
        level: Option<String>,
        /// Maximum number of entries to return
        limit: Option<usize>,
    },

    /// Clear the log buffer
    ClearLogs,

    /// Get current frame statistics
    GetFrameStats,

    /// Start recording performance data
    StartPerfRecording {
        /// Duration to record in milliseconds (0 = until stopped)
        duration_ms: u64,
    },

    /// Stop and get performance report
    GetPerfReport,

    /// Get the full UI tree from the embedded egui-mcp-client.
    /// Works on all platforms without AT-SPI (the app sends its AccessKit tree over IPC).
    GetUiTree,
}

/// Response types for IPC communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    /// Pong response to Ping
    Pong,

    /// Screenshot response
    Screenshot {
        /// Base64 encoded PNG data
        data: String,
        /// Image format (always "png")
        format: String,
    },

    /// Success response (for operations without data)
    Success,

    /// Error response
    Error { message: String },

    /// Log entries response
    Logs {
        /// Log entries (oldest first)
        entries: Vec<LogEntry>,
    },

    /// Frame statistics response
    FrameStatsResponse {
        /// Current frame statistics
        stats: FrameStats,
    },

    /// Performance report response
    PerfReportResponse {
        /// Performance report (None if not recording or no data)
        report: Option<PerfReport>,
    },

    /// UI tree response (from IPC-based AccessKit tree query, cross-platform)
    UiTreeResponse {
        /// The full UI tree from the egui application
        tree: UiTree,
    },
}

/// Protocol errors
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Message too large: {0} bytes")]
    MessageTooLarge(usize),
}

/// Maximum message size (1 MB)
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Read a length-prefixed message from a reader
pub async fn read_message<R: tokio::io::AsyncReadExt + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, ProtocolError> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(ProtocolError::ConnectionClosed);
        }
        Err(e) => return Err(e.into()),
    }

    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::MessageTooLarge(len));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    Ok(buf)
}

/// Write a length-prefixed message to a writer
pub async fn write_message<W: tokio::io::AsyncWriteExt + Unpin>(
    writer: &mut W,
    data: &[u8],
) -> Result<(), ProtocolError> {
    if data.len() > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::MessageTooLarge(data.len()));
    }

    let len = (data.len() as u32).to_be_bytes();
    writer.write_all(&len).await?;
    writer.write_all(data).await?;
    writer.flush().await?;
    Ok(())
}

/// Read and deserialize a request
pub async fn read_request<R: tokio::io::AsyncReadExt + Unpin>(
    reader: &mut R,
) -> Result<Request, ProtocolError> {
    let data = read_message(reader).await?;
    let request = serde_json::from_slice(&data)?;
    Ok(request)
}

/// Write and serialize a response
pub async fn write_response<W: tokio::io::AsyncWriteExt + Unpin>(
    writer: &mut W,
    response: &Response,
) -> Result<(), ProtocolError> {
    let data = serde_json::to_vec(response)?;
    write_message(writer, &data).await
}

/// Read and deserialize a response
pub async fn read_response<R: tokio::io::AsyncReadExt + Unpin>(
    reader: &mut R,
) -> Result<Response, ProtocolError> {
    let data = read_message(reader).await?;
    let response = serde_json::from_slice(&data)?;
    Ok(response)
}

/// Write and serialize a request
pub async fn write_request<W: tokio::io::AsyncWriteExt + Unpin>(
    writer: &mut W,
    request: &Request,
) -> Result<(), ProtocolError> {
    let data = serde_json::to_vec(request)?;
    write_message(writer, &data).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_request() {
        let req = Request::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Ping"));
    }

    #[test]
    fn test_serialize_response() {
        let resp = Response::Pong;
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Pong"));
    }

    #[test]
    fn test_default_socket_path() {
        let path = default_socket_path();
        assert!(path.to_string_lossy().contains("egui-mcp.sock"));
    }

    #[test]
    fn test_click_at_request() {
        let req = Request::ClickAt {
            x: 100.0,
            y: 200.0,
            button: MouseButton::Left,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ClickAt"));
        assert!(json.contains("100"));
    }

    #[test]
    fn test_keyboard_input_request() {
        let req = Request::KeyboardInput {
            key: "Enter".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("KeyboardInput"));
        assert!(json.contains("Enter"));
    }

    #[test]
    fn test_request_roundtrip_drag() {
        let req = Request::Drag {
            start_x: 10.0,
            start_y: 20.0,
            end_x: 100.0,
            end_y: 200.0,
            button: MouseButton::Left,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: Request = serde_json::from_str(&json).unwrap();
        if let Request::Drag {
            start_x,
            start_y,
            end_x,
            end_y,
            button,
        } = decoded
        {
            assert_eq!(start_x, 10.0);
            assert_eq!(start_y, 20.0);
            assert_eq!(end_x, 100.0);
            assert_eq!(end_y, 200.0);
            assert!(matches!(button, MouseButton::Left));
        } else {
            panic!("Expected Drag request");
        }
    }

    #[test]
    fn test_request_roundtrip_scroll() {
        let req = Request::Scroll {
            x: 50.0,
            y: 60.0,
            delta_x: -10.0,
            delta_y: 20.0,
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: Request = serde_json::from_str(&json).unwrap();
        if let Request::Scroll {
            x,
            y,
            delta_x,
            delta_y,
        } = decoded
        {
            assert_eq!(x, 50.0);
            assert_eq!(y, 60.0);
            assert_eq!(delta_x, -10.0);
            assert_eq!(delta_y, 20.0);
        } else {
            panic!("Expected Scroll request");
        }
    }

    #[test]
    fn test_response_roundtrip_screenshot() {
        let resp = Response::Screenshot {
            data: "base64data".to_string(),
            format: "png".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        if let Response::Screenshot { data, format } = decoded {
            assert_eq!(data, "base64data");
            assert_eq!(format, "png");
        } else {
            panic!("Expected Screenshot response");
        }
    }

    #[test]
    fn test_response_roundtrip_error() {
        let resp = Response::Error {
            message: "Something went wrong".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        if let Response::Error { message } = decoded {
            assert_eq!(message, "Something went wrong");
        } else {
            panic!("Expected Error response");
        }
    }

    #[test]
    fn test_node_info_serialization() {
        let node = NodeInfo {
            id: 42,
            role: "Button".to_string(),
            label: Some("Click me".to_string()),
            value: None,
            bounds: Some(Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 50.0,
            }),
            children: vec![1, 2, 3],
            toggled: Some(true),
            disabled: false,
            focused: true,
        };
        let json = serde_json::to_string(&node).unwrap();
        let decoded: NodeInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.role, "Button");
        assert_eq!(decoded.label, Some("Click me".to_string()));
        assert!(decoded.bounds.is_some());
        assert_eq!(decoded.children, vec![1, 2, 3]);
        assert_eq!(decoded.toggled, Some(true));
        assert!(!decoded.disabled);
        assert!(decoded.focused);
    }

    #[test]
    fn test_log_entry_serialization() {
        let entry = LogEntry {
            level: "INFO".to_string(),
            target: "my_app".to_string(),
            message: "Hello world".to_string(),
            timestamp_ms: 1234567890,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: LogEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.level, "INFO");
        assert_eq!(decoded.target, "my_app");
        assert_eq!(decoded.message, "Hello world");
        assert_eq!(decoded.timestamp_ms, 1234567890);
    }

    #[test]
    fn test_frame_stats_serialization() {
        let stats = FrameStats {
            fps: 60.0,
            frame_time_ms: 16.67,
            frame_time_min_ms: 15.0,
            frame_time_max_ms: 20.0,
            sample_count: 100,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: FrameStats = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.fps, 60.0);
        assert_eq!(decoded.sample_count, 100);
    }

    #[test]
    fn test_mouse_button_variants() {
        let buttons = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];
        for button in buttons {
            let json = serde_json::to_string(&button).unwrap();
            let decoded: MouseButton = serde_json::from_str(&json).unwrap();
            assert!(matches!(
                (&button, &decoded),
                (MouseButton::Left, MouseButton::Left)
                    | (MouseButton::Right, MouseButton::Right)
                    | (MouseButton::Middle, MouseButton::Middle)
            ));
        }
    }
}
