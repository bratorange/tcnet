//! Element action tool implementations (click_element, set_text, drag_element)

use super::{ToolResult, error_response, not_connected_error, parse_element_id, success_response};
use crate::ipc_client::IpcClient;
use egui_mcp_protocol::MouseButton;
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Click a UI element by its ID
pub async fn click_element(app_name: &str, ipc_client: &IpcClient, id_str: &str) -> ToolResult {
    let id = match parse_element_id(id_str) {
        Ok(id) => id,
        Err(e) => return e,
    };

    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return super::atspi_connection_error(e),
        };

        match client.click_element(app_name, id).await {
            Ok(true) => success_response(format!("Clicked element {}", id)),
            Ok(false) => error_response(
                "click_failed",
                format!("Element {} does not support click action", id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to click element: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = app_name;
        // Get the element bounds from the IPC tree and click its center
        match ipc_client.get_ui_tree().await {
            Ok(tree) => match tree.nodes.iter().find(|n| n.id == id) {
                Some(node) => match node.bounds {
                    Some(bounds) => {
                        let cx = bounds.x + bounds.width / 2.0;
                        let cy = bounds.y + bounds.height / 2.0;
                        match ipc_client.click_at(cx, cy, MouseButton::Left).await {
                            Ok(()) => success_response(format!(
                                "Clicked element {} at ({:.1}, {:.1})",
                                id, cx, cy
                            )),
                            Err(e) => error_response(
                                "click_error",
                                format!("Failed to click at center: {}", e),
                            ),
                        }
                    }
                    None => error_response(
                        "no_bounds",
                        format!("Element {} has no bounds — cannot compute click target", id),
                    ),
                },
                None => error_response("not_found", format!("Element with ID {} not found", id)),
            },
            Err(e) => error_response("ipc_error", format!("Failed to get UI tree: {}", e)),
        }
    }
}

/// Set text content of a text input element
pub async fn set_text(app_name: &str, id_str: &str, text: &str) -> ToolResult {
    let id = match parse_element_id(id_str) {
        Ok(id) => id,
        Err(e) => return e,
    };

    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return super::atspi_connection_error(e),
        };

        match client.set_text(app_name, id, text).await {
            Ok(true) => success_response(format!("Set text on element {}", id)),
            Ok(false) => error_response(
                "set_text_failed",
                format!("Element {} does not support text input", id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to set text: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id, text);
        error_response("not_available", "set_text requires AT-SPI on Linux.")
    }
}

/// Drag an element to a target position
pub async fn drag_element(
    app_name: &str,
    ipc_client: &IpcClient,
    source_id: &str,
    end_x: f32,
    end_y: f32,
    button: Option<&str>,
) -> ToolResult {
    let id = match parse_element_id(source_id) {
        Ok(id) => id,
        Err(e) => return e,
    };

    if !ipc_client.is_socket_available() {
        return not_connected_error();
    }

    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return super::atspi_connection_error(e),
        };

        // Get element bounds
        match client.get_bounds(app_name, id).await {
            Ok(Some(bounds)) => {
                let center_x = bounds.x + bounds.width / 2.0;
                let center_y = bounds.y + bounds.height / 2.0;

                let mouse_button = match button {
                    Some("right") => MouseButton::Right,
                    Some("middle") => MouseButton::Middle,
                    _ => MouseButton::Left,
                };

                match ipc_client
                    .drag(center_x, center_y, end_x, end_y, mouse_button)
                    .await
                {
                    Ok(()) => {
                        json!({
                            "success": true,
                            "message": format!("Dragged element {} from ({:.1}, {:.1}) to ({:.1}, {:.1})", id, center_x, center_y, end_x, end_y),
                            "start": {"x": center_x, "y": center_y},
                            "end": {"x": end_x, "y": end_y}
                        })
                        .to_string()
                    }
                    Err(e) => {
                        error_response("drag_error", format!("Failed to drag: {}", e))
                    }
                }
            }
            Ok(None) => error_response("no_bounds", format!("Element {} has no bounds", id)),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to get element bounds: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id, end_x, end_y, button);
        error_response("not_available", "drag_element requires AT-SPI on Linux.")
    }
}
