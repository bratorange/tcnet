//! Highlight tool implementations

use super::{ToolResult, error_response, not_connected_error, parse_element_id, success_response};
use crate::ipc_client::IpcClient;
use crate::utils::parse_hex_color;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Highlight a UI element
pub async fn highlight_element(
    app_name: &str,
    ipc_client: &IpcClient,
    id_str: &str,
    color: Option<&str>,
    duration_ms: Option<u64>,
) -> ToolResult {
    let id = match parse_element_id(id_str) {
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
                let color_array =
                    parse_hex_color(color.unwrap_or("#ff0000")).unwrap_or([255, 0, 0, 200]);
                let duration = duration_ms.unwrap_or(3000);

                match ipc_client
                    .highlight_element(
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                        color_array,
                        duration,
                    )
                    .await
                {
                    Ok(()) => {
                        success_response(format!("Highlighted element {} for {}ms", id, duration))
                    }
                    Err(e) => {
                        error_response("highlight_error", format!("Failed to highlight: {}", e))
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
        let _ = (app_name, id, color, duration_ms);
        error_response(
            "not_available",
            "highlight_element requires AT-SPI on Linux.",
        )
    }
}

/// Clear all highlights
pub async fn clear_highlights(ipc_client: &IpcClient) -> ToolResult {
    match ipc_client.clear_highlights().await {
        Ok(()) => success_response("All highlights cleared"),
        Err(e) => error_response("ipc_error", format!("Failed to clear highlights: {}", e)),
    }
}
