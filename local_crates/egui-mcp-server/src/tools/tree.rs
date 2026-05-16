//! UI tree tool implementations (get_ui_tree, find_by_label, find_by_role, get_element)

use super::{ToolResult, error_response, parse_element_id};
use crate::ipc_client::IpcClient;
use egui_mcp_protocol::{NodeInfo, UiTree};
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Get the UI tree from the connected egui application
pub async fn get_ui_tree(app_name: &str, ipc_client: &IpcClient) -> ToolResult {
    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return super::atspi_connection_error(e),
        };

        match client.get_ui_tree_by_app_name(app_name).await {
            Ok(Some(tree)) => {
                return serde_json::to_string_pretty(&tree).unwrap_or_else(|e| {
                    error_response(
                        "serialization_error",
                        format!("Failed to serialize UI tree: {}", e),
                    )
                });
            }
            Ok(None) => {
                tracing::info!("AT-SPI did not find any matching application");
            }
            Err(e) => {
                tracing::warn!("AT-SPI failed: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = app_name;

    get_tree_via_ipc(ipc_client).await
}

/// Find UI elements by their label text (substring match)
pub async fn find_by_label(
    app_name: &str,
    ipc_client: &IpcClient,
    pattern: &str,
    exact: bool,
) -> ToolResult {
    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return super::atspi_connection_error(e),
        };

        match client.find_by_label(app_name, pattern, exact).await {
            Ok(elements) => {
                return serde_json::to_string_pretty(&json!({
                    "count": elements.len(),
                    "elements": elements
                }))
                .unwrap_or_else(|e| {
                    error_response(
                        "serialization_error",
                        format!("Failed to serialize elements: {}", e),
                    )
                });
            }
            Err(e) => {
                tracing::warn!("AT-SPI find_by_label failed: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = app_name;

    match ipc_client.get_ui_tree().await {
        Ok(tree) => {
            let elements: Vec<&NodeInfo> = tree
                .nodes
                .iter()
                .filter(|n| {
                    let lbl = n.label.as_deref().unwrap_or("");
                    if exact { lbl == pattern } else { lbl.contains(pattern) }
                })
                .collect();
            serde_json::to_string_pretty(&json!({
                "count": elements.len(),
                "elements": elements
            }))
            .unwrap_or_else(|e| error_response("serialization_error", e.to_string()))
        }
        Err(e) => error_response("ipc_error", format!("Failed to get UI tree: {}", e)),
    }
}

/// Find UI elements by their role
pub async fn find_by_role(app_name: &str, ipc_client: &IpcClient, role: &str) -> ToolResult {
    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return super::atspi_connection_error(e),
        };

        match client.find_by_role(app_name, role).await {
            Ok(elements) => {
                return serde_json::to_string_pretty(&json!({
                    "count": elements.len(),
                    "elements": elements
                }))
                .unwrap_or_else(|e| {
                    error_response(
                        "serialization_error",
                        format!("Failed to serialize elements: {}", e),
                    )
                });
            }
            Err(e) => {
                tracing::warn!("AT-SPI find_by_role failed: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = app_name;

    let role_lower = role.to_lowercase();
    match ipc_client.get_ui_tree().await {
        Ok(tree) => {
            let elements: Vec<&NodeInfo> = tree
                .nodes
                .iter()
                .filter(|n| n.role.to_lowercase() == role_lower)
                .collect();
            serde_json::to_string_pretty(&json!({
                "count": elements.len(),
                "elements": elements
            }))
            .unwrap_or_else(|e| error_response("serialization_error", e.to_string()))
        }
        Err(e) => error_response("ipc_error", format!("Failed to get UI tree: {}", e)),
    }
}

/// Get detailed information about a specific UI element by its ID
pub async fn get_element(app_name: &str, ipc_client: &IpcClient, id_str: &str) -> ToolResult {
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

        match client.get_element(app_name, id).await {
            Ok(Some(element)) => {
                return serde_json::to_string_pretty(&element).unwrap_or_else(|e| {
                    error_response(
                        "serialization_error",
                        format!("Failed to serialize element: {}", e),
                    )
                });
            }
            Ok(None) => {
                return error_response("not_found", format!("Element with ID {} not found", id));
            }
            Err(e) => {
                tracing::warn!("AT-SPI get_element failed: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = app_name;

    match ipc_client.get_ui_tree().await {
        Ok(tree) => match tree.nodes.iter().find(|n| n.id == id) {
            Some(node) => serde_json::to_string_pretty(node)
                .unwrap_or_else(|e| error_response("serialization_error", e.to_string())),
            None => error_response("not_found", format!("Element with ID {} not found", id)),
        },
        Err(e) => error_response("ipc_error", format!("Failed to get UI tree: {}", e)),
    }
}

/// Get the UI tree via IPC and serialize it (non-Linux fallback, also used on Linux as secondary)
pub async fn get_tree_via_ipc(ipc_client: &IpcClient) -> ToolResult {
    match ipc_client.get_ui_tree().await {
        Ok(tree) => serde_json::to_string_pretty(&tree)
            .unwrap_or_else(|e| error_response("serialization_error", e.to_string())),
        Err(e) => error_response("ipc_error", format!("Failed to get UI tree via IPC: {}", e)),
    }
}

/// Get a UI tree from IPC, returning the UiTree directly (for internal use)
pub async fn fetch_tree(ipc_client: &IpcClient) -> Result<UiTree, String> {
    ipc_client
        .get_ui_tree()
        .await
        .map_err(|e| format!("IPC error: {}", e))
}
