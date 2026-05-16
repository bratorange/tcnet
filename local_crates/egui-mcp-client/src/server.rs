//! IPC server for handling MCP requests
//!
//! This server handles requests that require direct application access:
//! - Screenshots
//! - Coordinate-based input
//! - Keyboard input
//! - Scroll events

use crate::{McpClient, PendingInput};
use base64::Engine;
use egui_mcp_protocol::{ProtocolError, Request, Response, read_request, write_response};
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};

/// IPC server that listens for MCP requests
pub struct IpcServer;

impl IpcServer {
    /// Run the IPC server
    pub async fn run(client: McpClient) -> Result<(), ProtocolError> {
        let socket_path = client.socket_path().await;

        // Remove existing socket file if it exists
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        // Create parent directory if needed
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        tracing::info!("IPC server listening on {:?}", socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let client = client.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, client).await {
                            match e {
                                ProtocolError::ConnectionClosed => {
                                    tracing::debug!("Client disconnected");
                                }
                                _ => {
                                    tracing::error!("Connection error: {}", e);
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Handle a single connection
    async fn handle_connection(stream: UnixStream, client: McpClient) -> Result<(), ProtocolError> {
        let (mut reader, mut writer) = stream.into_split();

        loop {
            let request = read_request(&mut reader).await?;
            tracing::debug!("Received request: {:?}", request);

            let response = Self::handle_request(&request, &client).await;
            tracing::debug!("Sending response: {:?}", response);

            write_response(&mut writer, &response).await?;
        }
    }

    /// Handle a single request
    async fn handle_request(request: &Request, client: &McpClient) -> Response {
        match request {
            Request::Ping => Response::Pong,

            Request::TakeScreenshot => {
                // Request a screenshot and get a receiver (event-driven)
                let rx = client.request_screenshot().await;

                // Wait for the screenshot with timeout (no polling needed)
                match tokio::time::timeout(Duration::from_secs(5), rx).await {
                    Ok(Ok(data)) => {
                        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
                        Response::Screenshot {
                            data: encoded,
                            format: "png".to_string(),
                        }
                    }
                    Ok(Err(_)) => Response::Error {
                        message: "Screenshot request was cancelled".to_string(),
                    },
                    Err(_) => Response::Error {
                        message: "Screenshot timeout: the egui app did not provide a screenshot within 5 seconds".to_string(),
                    },
                }
            }

            Request::ClickAt { x, y, button } => {
                client
                    .queue_input(PendingInput::Click {
                        x: *x,
                        y: *y,
                        button: *button,
                    })
                    .await;
                Response::Success
            }

            Request::MoveMouse { x, y } => {
                client
                    .queue_input(PendingInput::MoveMouse { x: *x, y: *y })
                    .await;
                Response::Success
            }

            Request::KeyboardInput { key } => {
                client
                    .queue_input(PendingInput::Keyboard { key: key.clone() })
                    .await;
                Response::Success
            }

            Request::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                client
                    .queue_input(PendingInput::Scroll {
                        x: *x,
                        y: *y,
                        delta_x: *delta_x,
                        delta_y: *delta_y,
                    })
                    .await;
                Response::Success
            }

            Request::Drag {
                start_x,
                start_y,
                end_x,
                end_y,
                button,
            } => {
                client
                    .queue_input(PendingInput::Drag {
                        start_x: *start_x,
                        start_y: *start_y,
                        end_x: *end_x,
                        end_y: *end_y,
                        button: *button,
                    })
                    .await;
                Response::Success
            }

            Request::DoubleClick { x, y, button } => {
                client
                    .queue_input(PendingInput::DoubleClick {
                        x: *x,
                        y: *y,
                        button: *button,
                    })
                    .await;
                Response::Success
            }

            Request::TakeScreenshotRegion {
                x,
                y,
                width,
                height,
            } => {
                // Request a screenshot and get a receiver (event-driven)
                let rx = client.request_screenshot().await;

                // Wait for the screenshot with timeout (no polling needed)
                match tokio::time::timeout(Duration::from_secs(5), rx).await {
                    Ok(Ok(data)) => {
                        // Crop the screenshot to the specified region
                        match Self::crop_screenshot(&data, *x, *y, *width, *height) {
                            Ok(cropped) => {
                                let encoded =
                                    base64::engine::general_purpose::STANDARD.encode(&cropped);
                                Response::Screenshot {
                                    data: encoded,
                                    format: "png".to_string(),
                                }
                            }
                            Err(e) => Response::Error {
                                message: format!("Failed to crop screenshot: {}", e),
                            },
                        }
                    }
                    Ok(Err(_)) => Response::Error {
                        message: "Screenshot request was cancelled".to_string(),
                    },
                    Err(_) => Response::Error {
                        message: "Screenshot timeout: the egui app did not provide a screenshot within 5 seconds".to_string(),
                    },
                }
            }

            Request::HighlightElement {
                x,
                y,
                width,
                height,
                color,
                duration_ms,
            } => {
                let rect =
                    egui::Rect::from_min_size(egui::pos2(*x, *y), egui::vec2(*width, *height));
                let egui_color =
                    egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                let expires_at = if *duration_ms == 0 {
                    None
                } else {
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(*duration_ms))
                };
                client
                    .add_highlight(crate::Highlight {
                        rect,
                        color: egui_color,
                        expires_at,
                    })
                    .await;
                Response::Success
            }

            Request::ClearHighlights => {
                client.clear_highlights().await;
                Response::Success
            }

            Request::GetLogs { level, limit } => {
                let entries = client.get_logs(level.as_deref(), *limit).await;
                Response::Logs { entries }
            }

            Request::ClearLogs => {
                client.clear_logs().await;
                Response::Success
            }

            Request::GetFrameStats => {
                let stats = client.get_frame_stats().await;
                Response::FrameStatsResponse { stats }
            }

            Request::StartPerfRecording { duration_ms } => {
                client.start_perf_recording(*duration_ms).await;
                Response::Success
            }

            Request::GetPerfReport => {
                let report = client.get_perf_report().await;
                Response::PerfReportResponse { report }
            }

            Request::GetUiTree => {
                let tree = client.get_ui_tree().await;
                Response::UiTreeResponse { tree }
            }
        }
    }

    /// Crop a PNG screenshot to the specified region
    fn crop_screenshot(
        png_data: &[u8],
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<Vec<u8>, String> {
        use image::GenericImageView;
        use std::io::Cursor;

        let x = x as u32;
        let y = y as u32;
        let width = width as u32;
        let height = height as u32;

        // Load image from PNG data
        let img = image::load_from_memory(png_data)
            .map_err(|e| format!("Failed to load image: {}", e))?;

        // Validate crop region
        let (img_width, img_height) = img.dimensions();
        if x >= img_width || y >= img_height {
            return Err(format!(
                "Crop region starts outside image bounds. Image: {}x{}, Region start: ({}, {})",
                img_width, img_height, x, y
            ));
        }

        // Clamp dimensions to image bounds
        let clamped_w = width.min(img_width.saturating_sub(x));
        let clamped_h = height.min(img_height.saturating_sub(y));

        if clamped_w == 0 || clamped_h == 0 {
            return Err("Crop region has zero width or height".to_string());
        }

        // Crop the image
        let cropped = img.crop_imm(x, y, clamped_w, clamped_h);

        // Encode back to PNG
        let mut buf = Vec::new();
        cropped
            .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .map_err(|e| format!("Failed to encode PNG: {}", e))?;

        Ok(buf)
    }
}
