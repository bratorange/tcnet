//! Component interface tool implementations (get_bounds, focus_element, scroll_to_element)

use super::{ToolResult, error_response, parse_element_id, success_response};
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Get the bounding box of a UI element
pub async fn get_bounds(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.get_bounds(app_name, id).await {
            Ok(Some(bounds)) => json!({
                "x": bounds.x,
                "y": bounds.y,
                "width": bounds.width,
                "height": bounds.height
            })
            .to_string(),
            Ok(None) => error_response("no_bounds", format!("Element {} has no bounds", id)),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to get element bounds: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "get_bounds requires AT-SPI on Linux.")
    }
}

/// Focus a UI element by ID
pub async fn focus_element(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.focus_element(app_name, id).await {
            Ok(true) => success_response(format!("Focused element {}", id)),
            Ok(false) => error_response(
                "focus_failed",
                format!("Element {} could not be focused", id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to focus element: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "focus_element requires AT-SPI on Linux.")
    }
}

/// Scroll a UI element into view
pub async fn scroll_to_element(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.scroll_to_element(app_name, id).await {
            Ok(true) => success_response(format!("Scrolled element {} into view", id)),
            Ok(false) => error_response(
                "scroll_failed",
                format!("Element {} could not be scrolled into view", id),
            ),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to scroll element into view: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response(
            "not_available",
            "scroll_to_element requires AT-SPI on Linux.",
        )
    }
}
