//! Selection interface tool implementations

use super::{ToolResult, error_response, parse_element_id, success_response};
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Select an item by index in a selection container
pub async fn select_item(app_name: &str, id_str: &str, index: i32) -> ToolResult {
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

        match client.select_item(app_name, id, index).await {
            Ok(true) => success_response(format!("Selected item {} in element {}", index, id)),
            Ok(false) => error_response(
                "selection_failed",
                format!("Failed to select item {} in element {}", index, id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to select item: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id, index);
        error_response("not_available", "select_item requires AT-SPI on Linux.")
    }
}

/// Deselect an item by index in a selection container
pub async fn deselect_item(app_name: &str, id_str: &str, index: i32) -> ToolResult {
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

        match client.deselect_item(app_name, id, index).await {
            Ok(true) => success_response(format!("Deselected item {} in element {}", index, id)),
            Ok(false) => error_response(
                "deselection_failed",
                format!("Failed to deselect item {} in element {}", index, id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to deselect item: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id, index);
        error_response("not_available", "deselect_item requires AT-SPI on Linux.")
    }
}

/// Get the number of selected items in a selection container
pub async fn get_selected_count(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.get_selected_count(app_name, id).await {
            Ok(count) => json!({
                "id": id.to_string(),
                "count": count
            })
            .to_string(),
            Err(e) => error_response(
                "atspi_error",
                format!("Failed to get selected count: {}", e),
            ),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response(
            "not_available",
            "get_selected_count requires AT-SPI on Linux.",
        )
    }
}

/// Select all items in a selection container
pub async fn select_all(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.select_all(app_name, id).await {
            Ok(true) => success_response(format!("Selected all items in element {}", id)),
            Ok(false) => error_response(
                "selection_failed",
                format!("Failed to select all items in element {}", id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to select all: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "select_all requires AT-SPI on Linux.")
    }
}

/// Clear all selections in a selection container
pub async fn clear_selection(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.clear_selection(app_name, id).await {
            Ok(true) => success_response(format!("Cleared selection in element {}", id)),
            Ok(false) => error_response(
                "clear_failed",
                format!("Failed to clear selection in element {}", id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to clear selection: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "clear_selection requires AT-SPI on Linux.")
    }
}
