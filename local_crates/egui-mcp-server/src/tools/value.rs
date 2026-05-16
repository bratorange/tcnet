//! Value interface tool implementations (get_value, set_value)

use super::{ToolResult, error_response, parse_element_id, success_response};
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Get the current value of a value element (slider, progress bar, etc.)
pub async fn get_value(app_name: &str, id_str: &str) -> ToolResult {
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

        match client.get_value(app_name, id).await {
            Ok(Some(info)) => json!({
                "current": info.current,
                "min": info.minimum,
                "max": info.maximum,
                "step": info.increment
            })
            .to_string(),
            Ok(None) => error_response(
                "no_value",
                format!("Element {} does not support Value interface", id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to get value: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id);
        error_response("not_available", "get_value requires AT-SPI on Linux.")
    }
}

/// Set the value of a value element (slider, etc.)
pub async fn set_value(app_name: &str, id_str: &str, value: f64) -> ToolResult {
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

        match client.set_value(app_name, id, value).await {
            Ok(true) => success_response(format!("Set value of element {} to {}", id, value)),
            Ok(false) => error_response(
                "set_value_failed",
                format!("Element {} does not support setting value", id),
            ),
            Err(e) => error_response("atspi_error", format!("Failed to set value: {}", e)),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id, value);
        error_response("not_available", "set_value requires AT-SPI on Linux.")
    }
}
