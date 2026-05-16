//! Text interface tool implementations

use super::{ToolResult, error_response, parse_element_id, success_response};
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Get text content of an element
pub async fn get_text(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.get_text(app_name, id).await {
            Ok(Some(info)) => json!({
                "text": info.text,
                "length": info.length,
                "caret_offset": info.caret_offset
            })
            .to_string(),
            Ok(None) => error_response(
                "no_text",
                format!("Element {} does not have text content", id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to get text: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "get_text requires AT-SPI on Linux.")
    }
}

/// Get text selection range
pub async fn get_text_selection(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.get_text_selection(app_name, id).await {
            Ok(Some(selection)) => json!({
                "start": selection.start,
                "end": selection.end,
                "has_selection": selection.start != selection.end
            })
            .to_string(),
            Ok(None) => json!({
                "start": -1,
                "end": -1,
                "has_selection": false,
                "message": "Element has no focus or no text interface"
            })
            .to_string(),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to get text selection: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response(
            "not_available",
            "get_text_selection requires AT-SPI on Linux.",
        )
    }
}

/// Set text selection range
pub async fn set_text_selection(app_name: &str, id_str: &str, start: i32, end: i32) -> ToolResult {
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

        match client.set_text_selection(app_name, id, start, end).await {
            Ok(true) => success_response(format!(
                "Set selection on element {} from {} to {}",
                id, start, end
            )),
            Ok(false) => error_response(
                "selection_failed",
                format!("Failed to set selection on element {}", id),
            ),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to set text selection: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id, start, end);
        error_response(
            "not_available",
            "set_text_selection requires AT-SPI on Linux.",
        )
    }
}

/// Get caret position
pub async fn get_caret_position(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.get_caret_position(app_name, id).await {
            Ok(offset) => json!({
                "offset": offset,
                "has_focus": offset >= 0
            })
            .to_string(),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to get caret position: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response(
            "not_available",
            "get_caret_position requires AT-SPI on Linux.",
        )
    }
}

/// Set caret position
pub async fn set_caret_position(app_name: &str, id_str: &str, offset: i32) -> ToolResult {
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

        match client.set_caret_position(app_name, id, offset).await {
            Ok(true) => success_response(format!(
                "Set caret position on element {} to {}",
                id, offset
            )),
            Ok(false) => error_response(
                "caret_failed",
                format!("Failed to set caret position on element {}", id),
            ),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to set caret position: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id, offset);
        error_response(
            "not_available",
            "set_caret_position requires AT-SPI on Linux.",
        )
    }
}
