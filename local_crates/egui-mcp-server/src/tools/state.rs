//! State interface tool implementations (is_visible, is_enabled, is_focused, is_checked)

use super::{ToolResult, error_response, parse_element_id};
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Check if an element is visible
pub async fn is_visible(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.is_visible(app_name, id).await {
            Ok(visible) => json!({
                "id": id.to_string(),
                "visible": visible
            })
            .to_string(),
            Err(e) => error_response("atspi_error", format!("Failed to check visibility: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "is_visible requires AT-SPI on Linux.")
    }
}

/// Check if an element is enabled
pub async fn is_enabled(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.is_enabled(app_name, id).await {
            Ok(enabled) => json!({
                "id": id.to_string(),
                "enabled": enabled
            })
            .to_string(),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to check enabled state: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "is_enabled requires AT-SPI on Linux.")
    }
}

/// Check if an element is focused
pub async fn is_focused(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.is_focused(app_name, id).await {
            Ok(focused) => json!({
                "id": id.to_string(),
                "focused": focused
            })
            .to_string(),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to check focused state: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "is_focused requires AT-SPI on Linux.")
    }
}

/// Check if an element is checked or pressed
pub async fn is_checked(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.is_checked(app_name, id).await {
            Ok(Some(checked)) => json!({
                "id": id.to_string(),
                "checked": checked
            })
            .to_string(),
            Ok(None) => json!({
                "id": id.to_string(),
                "checked": null,
                "message": "Element is not a checkable type"
            })
            .to_string(),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to check checked state: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "is_checked requires AT-SPI on Linux.")
    }
}
