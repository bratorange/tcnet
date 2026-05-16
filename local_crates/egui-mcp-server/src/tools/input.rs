//! IPC input tool implementations (click_at, keyboard_input, scroll, hover, drag, double_click)

use super::{ToolResult, error_response, not_connected_error, success_response};
use crate::ipc_client::IpcClient;
use egui_mcp_protocol::MouseButton;

/// Click at specific coordinates
pub async fn click_at(ipc_client: &IpcClient, x: f32, y: f32, button: Option<&str>) -> ToolResult {
    if !ipc_client.is_socket_available() {
        return not_connected_error();
    }

    let mouse_button = match button {
        Some("right") => MouseButton::Right,
        Some("middle") => MouseButton::Middle,
        _ => MouseButton::Left,
    };

    match ipc_client.click_at(x, y, mouse_button).await {
        Ok(()) => success_response(format!("Clicked at ({}, {})", x, y)),
        Err(e) => error_response("click_error", format!("Failed to click: {}", e)),
    }
}

/// Send keyboard input
pub async fn keyboard_input(ipc_client: &IpcClient, key: &str) -> ToolResult {
    if !ipc_client.is_socket_available() {
        return not_connected_error();
    }

    match ipc_client.keyboard_input(key).await {
        Ok(()) => success_response(format!("Sent key: {}", key)),
        Err(e) => error_response(
            "keyboard_error",
            format!("Failed to send keyboard input: {}", e),
        ),
    }
}

/// Scroll at specific coordinates
pub async fn scroll(
    ipc_client: &IpcClient,
    x: f32,
    y: f32,
    delta_x: Option<f32>,
    delta_y: Option<f32>,
) -> ToolResult {
    if !ipc_client.is_socket_available() {
        return not_connected_error();
    }

    let dx = delta_x.unwrap_or(0.0);
    let dy = delta_y.unwrap_or(0.0);

    match ipc_client.scroll(x, y, dx, dy).await {
        Ok(()) => success_response(format!(
            "Scrolled at ({}, {}) with delta ({}, {})",
            x, y, dx, dy
        )),
        Err(e) => error_response("scroll_error", format!("Failed to scroll: {}", e)),
    }
}

/// Move mouse to specific coordinates (hover)
pub async fn hover(ipc_client: &IpcClient, x: f32, y: f32) -> ToolResult {
    if !ipc_client.is_socket_available() {
        return not_connected_error();
    }

    match ipc_client.move_mouse(x, y).await {
        Ok(()) => success_response(format!("Moved mouse to ({}, {})", x, y)),
        Err(e) => error_response("hover_error", format!("Failed to move mouse: {}", e)),
    }
}

/// Drag from one point to another
pub async fn drag(
    ipc_client: &IpcClient,
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
    button: Option<&str>,
) -> ToolResult {
    if !ipc_client.is_socket_available() {
        return not_connected_error();
    }

    let mouse_button = match button {
        Some("right") => MouseButton::Right,
        Some("middle") => MouseButton::Middle,
        _ => MouseButton::Left,
    };

    match ipc_client
        .drag(start_x, start_y, end_x, end_y, mouse_button)
        .await
    {
        Ok(()) => success_response(format!(
            "Dragged from ({}, {}) to ({}, {})",
            start_x, start_y, end_x, end_y
        )),
        Err(e) => error_response("drag_error", format!("Failed to drag: {}", e)),
    }
}

/// Double click at specific coordinates
pub async fn double_click(
    ipc_client: &IpcClient,
    x: f32,
    y: f32,
    button: Option<&str>,
) -> ToolResult {
    if !ipc_client.is_socket_available() {
        return not_connected_error();
    }

    let mouse_button = match button {
        Some("right") => MouseButton::Right,
        Some("middle") => MouseButton::Middle,
        _ => MouseButton::Left,
    };

    match ipc_client.double_click(x, y, mouse_button).await {
        Ok(()) => success_response(format!("Double clicked at ({}, {})", x, y)),
        Err(e) => error_response(
            "double_click_error",
            format!("Failed to double click: {}", e),
        ),
    }
}
