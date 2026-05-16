//! Wait tool implementations

use super::{ToolResult, error_response};
use crate::constants::{DEFAULT_WAIT_TIMEOUT_MS, WAIT_POLL_INTERVAL_MS};
use crate::ipc_client::IpcClient;
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Wait for a UI element to appear or disappear
pub async fn wait_for_element(
    app_name: &str,
    ipc_client: &IpcClient,
    pattern: &str,
    appear: bool,
    timeout_ms: u64,
) -> ToolResult {
    #[cfg(target_os = "linux")]
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);

        loop {
            let client = match AtspiClient::new().await {
                Ok(c) => c,
                Err(e) => return super::atspi_connection_error(e),
            };

            match client.find_by_label(app_name, pattern, false).await {
                Ok(elements) => {
                    let found = !elements.is_empty();
                    if found == appear {
                        return json!({
                            "success": true,
                            "found": found,
                            "pattern": pattern,
                            "count": elements.len(),
                            "elements": if appear { elements } else { vec![] }
                        })
                        .to_string();
                    }
                }
                Err(e) => {
                    tracing::debug!("AT-SPI search failed during wait: {}", e);
                }
            }

            if std::time::Instant::now() >= deadline {
                return json!({
                    "success": false,
                    "timeout": true,
                    "pattern": pattern,
                    "waited_for": if appear { "appear" } else { "disappear" },
                    "message": format!(
                        "Timeout after {}ms waiting for element to {}",
                        timeout_ms,
                        if appear { "appear" } else { "disappear" }
                    )
                })
                .to_string();
            }

            tokio::time::sleep(std::time::Duration::from_millis(WAIT_POLL_INTERVAL_MS)).await;
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = app_name;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);

        loop {
            match ipc_client.get_ui_tree().await {
                Ok(tree) => {
                    let elements: Vec<_> = tree
                        .nodes
                        .iter()
                        .filter(|n| {
                            n.label.as_deref().unwrap_or("").contains(pattern)
                        })
                        .cloned()
                        .collect();
                    let found = !elements.is_empty();
                    if found == appear {
                        return json!({
                            "success": true,
                            "found": found,
                            "pattern": pattern,
                            "count": elements.len(),
                            "elements": if appear { elements } else { vec![] }
                        })
                        .to_string();
                    }
                }
                Err(e) => {
                    tracing::debug!("IPC tree fetch failed during wait: {}", e);
                }
            }

            if std::time::Instant::now() >= deadline {
                return json!({
                    "success": false,
                    "timeout": true,
                    "pattern": pattern,
                    "waited_for": if appear { "appear" } else { "disappear" },
                    "message": format!(
                        "Timeout after {}ms waiting for element to {}",
                        timeout_ms,
                        if appear { "appear" } else { "disappear" }
                    )
                })
                .to_string();
            }

            tokio::time::sleep(std::time::Duration::from_millis(WAIT_POLL_INTERVAL_MS)).await;
        }
    }
}

/// Wait for a UI element's state to reach an expected value
pub async fn wait_for_state(
    app_name: &str,
    ipc_client: &IpcClient,
    id_str: &str,
    state: &str,
    expected: bool,
    timeout_ms: u64,
) -> ToolResult {
    let id = match super::parse_element_id(id_str) {
        Ok(id) => id,
        Err(e) => return e,
    };

    #[cfg(target_os = "linux")]
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);

        loop {
            let client = match AtspiClient::new().await {
                Ok(c) => c,
                Err(e) => return super::atspi_connection_error(e),
            };

            let current_state = match state {
                "visible" => client.is_visible(app_name, id).await.ok(),
                "enabled" => client.is_enabled(app_name, id).await.ok(),
                "focused" => client.is_focused(app_name, id).await.ok(),
                "checked" => client.is_checked(app_name, id).await.ok().flatten(),
                _ => {
                    return error_response(
                        "invalid_state",
                        format!(
                            "Invalid state '{}'. Must be one of: visible, enabled, focused, checked",
                            state
                        ),
                    );
                }
            };

            if let Some(current) = current_state
                && current == expected
            {
                return json!({
                    "success": true,
                    "id": id.to_string(),
                    "state": state,
                    "value": current
                })
                .to_string();
            }

            if std::time::Instant::now() >= deadline {
                return json!({
                    "success": false,
                    "timeout": true,
                    "id": id.to_string(),
                    "state": state,
                    "expected": expected,
                    "message": format!(
                        "Timeout after {}ms waiting for {} to be {}",
                        timeout_ms, state, expected
                    )
                })
                .to_string();
            }

            tokio::time::sleep(std::time::Duration::from_millis(WAIT_POLL_INTERVAL_MS)).await;
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = app_name;

        if !matches!(state, "visible" | "enabled" | "focused" | "checked") {
            return error_response(
                "invalid_state",
                format!(
                    "Invalid state '{}'. Must be one of: visible, enabled, focused, checked",
                    state
                ),
            );
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);

        loop {
            let current_state = match ipc_client.get_ui_tree().await {
                Ok(tree) => {
                    tree.nodes.iter().find(|n| n.id == id).map(|node| {
                        match state {
                            "visible" => true, // all nodes in AccessKit tree are visible
                            "enabled" => !node.disabled,
                            "focused" => node.focused,
                            "checked" => node.toggled.unwrap_or(false),
                            _ => false,
                        }
                    })
                }
                Err(e) => {
                    tracing::debug!("IPC tree fetch failed during wait_for_state: {}", e);
                    None
                }
            };

            if let Some(current) = current_state {
                if current == expected {
                    return json!({
                        "success": true,
                        "id": id.to_string(),
                        "state": state,
                        "value": current
                    })
                    .to_string();
                }
            }

            if std::time::Instant::now() >= deadline {
                return json!({
                    "success": false,
                    "timeout": true,
                    "id": id.to_string(),
                    "state": state,
                    "expected": expected,
                    "message": format!(
                        "Timeout after {}ms waiting for {} to be {}",
                        timeout_ms, state, expected
                    )
                })
                .to_string();
            }

            tokio::time::sleep(std::time::Duration::from_millis(WAIT_POLL_INTERVAL_MS)).await;
        }
    }
}

/// Default timeout in milliseconds
pub const DEFAULT_TIMEOUT_MS: u64 = DEFAULT_WAIT_TIMEOUT_MS;
