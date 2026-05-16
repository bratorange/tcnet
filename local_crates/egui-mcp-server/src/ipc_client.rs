//! IPC client for connecting to egui applications
//!
//! This client handles features that require direct application access:
//! - Screenshots
//! - Coordinate-based input (clicks, drags)
//! - Keyboard input
//! - Scroll events
//!
//! Note: UI tree access and element-based interactions are handled via AT-SPI.

use egui_mcp_protocol::{
    FrameStats, LogEntry, MouseButton, PerfReport, ProtocolError, Request, Response, UiTree,
    default_socket_path, read_response, write_request,
};
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;

/// Cached connection to the egui application
struct CachedConnection {
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
}

/// IPC client for communicating with egui applications
///
/// This client maintains a cached connection to reduce connection overhead.
/// If the connection fails, it automatically reconnects on the next request.
pub struct IpcClient {
    socket_path: PathBuf,
    connection: Mutex<Option<CachedConnection>>,
}

impl IpcClient {
    /// Create a new IPC client with default socket path
    pub fn new() -> Self {
        Self::with_socket_path(default_socket_path())
    }

    /// Create a new IPC client with a custom socket path
    pub fn with_socket_path(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            connection: Mutex::new(None),
        }
    }

    /// Get or create a connection to the egui application
    async fn get_connection(
        &self,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<CachedConnection>>, ProtocolError> {
        let mut guard = self.connection.lock().await;
        if guard.is_none() {
            let stream = UnixStream::connect(&self.socket_path).await?;
            let (reader, writer) = stream.into_split();
            *guard = Some(CachedConnection { reader, writer });
        }
        Ok(guard)
    }

    /// Connect to the egui application and send a request
    ///
    /// This method reuses an existing connection if available.
    /// If the connection fails, it automatically reconnects and retries once.
    async fn send_request(&self, request: &Request) -> Result<Response, ProtocolError> {
        // Try with existing or new connection
        let result = self.try_send_request(request).await;

        match result {
            Ok(response) => Ok(response),
            Err(_) => {
                // Connection failed, clear it and try once more with a fresh connection
                *self.connection.lock().await = None;
                self.try_send_request(request).await
            }
        }
    }

    /// Try to send a request using the cached connection
    async fn try_send_request(&self, request: &Request) -> Result<Response, ProtocolError> {
        let mut guard = self.get_connection().await?;
        let conn = guard.as_mut().ok_or_else(|| {
            ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "No connection available",
            ))
        })?;

        write_request(&mut conn.writer, request).await?;
        let response = read_response(&mut conn.reader).await?;

        Ok(response)
    }

    /// Ping the egui application
    pub async fn ping(&self) -> Result<bool, ProtocolError> {
        let response = self.send_request(&Request::Ping).await?;
        match response {
            Response::Pong => Ok(true),
            Response::Error { message } => {
                tracing::error!("Ping error: {}", message);
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// Take a screenshot of the egui application
    /// Returns (base64_data, format)
    pub async fn take_screenshot(&self) -> Result<(String, String), ProtocolError> {
        let response = self.send_request(&Request::TakeScreenshot).await?;
        match response {
            Response::Screenshot { data, format } => Ok((data, format)),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Click at specific coordinates
    pub async fn click_at(&self, x: f32, y: f32, button: MouseButton) -> Result<(), ProtocolError> {
        let response = self
            .send_request(&Request::ClickAt { x, y, button })
            .await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Send keyboard input
    pub async fn keyboard_input(&self, key: &str) -> Result<(), ProtocolError> {
        let response = self
            .send_request(&Request::KeyboardInput {
                key: key.to_string(),
            })
            .await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Scroll at specific coordinates
    pub async fn scroll(
        &self,
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), ProtocolError> {
        let response = self
            .send_request(&Request::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            })
            .await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Move mouse to specific coordinates
    pub async fn move_mouse(&self, x: f32, y: f32) -> Result<(), ProtocolError> {
        let response = self.send_request(&Request::MoveMouse { x, y }).await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Drag from one position to another
    pub async fn drag(
        &self,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        button: MouseButton,
    ) -> Result<(), ProtocolError> {
        let response = self
            .send_request(&Request::Drag {
                start_x,
                start_y,
                end_x,
                end_y,
                button,
            })
            .await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Double click at specific coordinates
    pub async fn double_click(
        &self,
        x: f32,
        y: f32,
        button: MouseButton,
    ) -> Result<(), ProtocolError> {
        let response = self
            .send_request(&Request::DoubleClick { x, y, button })
            .await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Take a screenshot of a specific region of the egui application
    /// Returns (base64_data, format)
    #[allow(dead_code)]
    pub async fn take_screenshot_region(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(String, String), ProtocolError> {
        let response = self
            .send_request(&Request::TakeScreenshotRegion {
                x,
                y,
                width,
                height,
            })
            .await?;
        match response {
            Response::Screenshot { data, format } => Ok((data, format)),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Highlight an element with a colored border
    pub async fn highlight_element(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [u8; 4],
        duration_ms: u64,
    ) -> Result<(), ProtocolError> {
        let response = self
            .send_request(&Request::HighlightElement {
                x,
                y,
                width,
                height,
                color,
                duration_ms,
            })
            .await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Clear all highlights
    pub async fn clear_highlights(&self) -> Result<(), ProtocolError> {
        let response = self.send_request(&Request::ClearHighlights).await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Check if the socket file exists (quick check without connecting)
    pub fn is_socket_available(&self) -> bool {
        self.socket_path.exists()
    }

    /// Get log entries from the egui application
    pub async fn get_logs(
        &self,
        level: Option<String>,
        limit: Option<usize>,
    ) -> Result<Vec<LogEntry>, ProtocolError> {
        let response = self
            .send_request(&Request::GetLogs { level, limit })
            .await?;
        match response {
            Response::Logs { entries } => Ok(entries),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Clear all logs in the egui application
    pub async fn clear_logs(&self) -> Result<(), ProtocolError> {
        let response = self.send_request(&Request::ClearLogs).await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Get current frame statistics
    pub async fn get_frame_stats(&self) -> Result<FrameStats, ProtocolError> {
        let response = self.send_request(&Request::GetFrameStats).await?;
        match response {
            Response::FrameStatsResponse { stats } => Ok(stats),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Start recording performance data
    pub async fn start_perf_recording(&self, duration_ms: u64) -> Result<(), ProtocolError> {
        let response = self
            .send_request(&Request::StartPerfRecording { duration_ms })
            .await?;
        match response {
            Response::Success => Ok(()),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Get performance report (stops recording)
    pub async fn get_perf_report(&self) -> Result<Option<PerfReport>, ProtocolError> {
        let response = self.send_request(&Request::GetPerfReport).await?;
        match response {
            Response::PerfReportResponse { report } => Ok(report),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }

    /// Get the full UI tree from the connected egui application (cross-platform, IPC-based)
    pub async fn get_ui_tree(&self) -> Result<UiTree, ProtocolError> {
        let response = self.send_request(&Request::GetUiTree).await?;
        match response {
            Response::UiTreeResponse { tree } => Ok(tree),
            Response::Error { message } => Err(ProtocolError::Io(std::io::Error::other(message))),
            _ => Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unexpected response",
            ))),
        }
    }
}

impl Default for IpcClient {
    fn default() -> Self {
        Self::new()
    }
}
