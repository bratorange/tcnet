//! Snapshot tool implementations

use super::{ToolResult, error_response};
use crate::ipc_client::IpcClient;
use crate::utils::compute_tree_diff;
use serde_json::json;
use std::collections::HashMap;
use std::sync::RwLock;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Stored snapshot data (serialized UiTree)
pub type SnapshotStore = std::sync::Arc<RwLock<HashMap<String, String>>>;

/// Save current UI tree state as a named snapshot
pub async fn save_snapshot(
    app_name: &str,
    ipc_client: &IpcClient,
    snapshots: &SnapshotStore,
    name: &str,
) -> ToolResult {
    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return super::atspi_connection_error(e),
        };

        match client.get_ui_tree_by_app_name(app_name).await {
            Ok(Some(tree)) => {
                let json = serde_json::to_string(&tree).unwrap_or_default();
                let node_count = tree.nodes.len();

                if let Ok(mut store) = snapshots.write() {
                    store.insert(name.to_string(), json);
                }

                return json!({
                    "success": true,
                    "name": name,
                    "node_count": node_count
                })
                .to_string();
            }
            Ok(None) => {
                tracing::info!("AT-SPI did not find any matching application for snapshot");
            }
            Err(e) => {
                tracing::warn!("AT-SPI failed for snapshot: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = app_name;

    match ipc_client.get_ui_tree().await {
        Ok(tree) => {
            let node_count = tree.nodes.len();
            let json = serde_json::to_string(&tree).unwrap_or_default();
            if let Ok(mut store) = snapshots.write() {
                store.insert(name.to_string(), json);
            }
            json!({
                "success": true,
                "name": name,
                "node_count": node_count
            })
            .to_string()
        }
        Err(e) => error_response("ipc_error", format!("Failed to get UI tree: {}", e)),
    }
}

/// Load a saved snapshot
pub fn load_snapshot(snapshots: &SnapshotStore, name: &str) -> ToolResult {
    if let Ok(store) = snapshots.read() {
        if let Some(json) = store.get(name) {
            match serde_json::from_str::<egui_mcp_protocol::UiTree>(json) {
                Ok(tree) => json!({
                    "success": true,
                    "name": name,
                    "node_count": tree.nodes.len(),
                    "tree": tree
                })
                .to_string(),
                Err(e) => error_response("parse_error", format!("Failed to parse snapshot: {}", e)),
            }
        } else {
            error_response("not_found", format!("Snapshot '{}' not found", name))
        }
    } else {
        error_response("lock_error", "Failed to acquire snapshot lock")
    }
}

/// Compare two saved snapshots
pub fn diff_snapshots(snapshots: &SnapshotStore, name_a: &str, name_b: &str) -> ToolResult {
    let store = match snapshots.read() {
        Ok(s) => s,
        Err(_) => return error_response("lock_error", "Failed to acquire snapshot lock"),
    };

    let json_a = match store.get(name_a) {
        Some(j) => j,
        None => return error_response("not_found", format!("Snapshot '{}' not found", name_a)),
    };

    let json_b = match store.get(name_b) {
        Some(j) => j,
        None => return error_response("not_found", format!("Snapshot '{}' not found", name_b)),
    };

    let tree_a: egui_mcp_protocol::UiTree = match serde_json::from_str(json_a) {
        Ok(t) => t,
        Err(e) => {
            return error_response(
                "parse_error",
                format!("Failed to parse snapshot '{}': {}", name_a, e),
            );
        }
    };

    let tree_b: egui_mcp_protocol::UiTree = match serde_json::from_str(json_b) {
        Ok(t) => t,
        Err(e) => {
            return error_response(
                "parse_error",
                format!("Failed to parse snapshot '{}': {}", name_b, e),
            );
        }
    };

    let diff = compute_tree_diff(&tree_a, &tree_b);
    // compute_tree_diff returns a serde_json::Value with all the fields we need
    json!({
        "name_a": name_a,
        "name_b": name_b,
        "added": diff["added"],
        "removed": diff["removed"],
        "modified": diff["modified"],
        "added_count": diff["added_count"],
        "removed_count": diff["removed_count"],
        "modified_count": diff["modified_count"]
    })
    .to_string()
}

/// Compare current UI tree state with a saved snapshot
pub async fn diff_current(
    app_name: &str,
    ipc_client: &IpcClient,
    snapshots: &SnapshotStore,
    name: &str,
) -> ToolResult {
    // Get saved snapshot
    let saved_json = {
        let store = match snapshots.read() {
            Ok(s) => s,
            Err(_) => return error_response("lock_error", "Failed to acquire snapshot lock"),
        };
        match store.get(name) {
            Some(j) => j.clone(),
            None => return error_response("not_found", format!("Snapshot '{}' not found", name)),
        }
    };

    let saved_tree: egui_mcp_protocol::UiTree = match serde_json::from_str(&saved_json) {
        Ok(t) => t,
        Err(e) => {
            return error_response(
                "parse_error",
                format!("Failed to parse snapshot '{}': {}", name, e),
            );
        }
    };

    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return super::atspi_connection_error(e),
        };

        match client.get_ui_tree_by_app_name(app_name).await {
            Ok(Some(current_tree)) => {
                let diff = compute_tree_diff(&saved_tree, &current_tree);
                return json!({
                    "snapshot_name": name,
                    "added": diff["added"],
                    "removed": diff["removed"],
                    "modified": diff["modified"],
                    "added_count": diff["added_count"],
                    "removed_count": diff["removed_count"],
                    "modified_count": diff["modified_count"]
                })
                .to_string();
            }
            Ok(None) => {
                tracing::info!("AT-SPI did not find any matching application for diff_current");
            }
            Err(e) => {
                tracing::warn!("AT-SPI failed for diff_current: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = app_name;

    match ipc_client.get_ui_tree().await {
        Ok(current_tree) => {
            let diff = compute_tree_diff(&saved_tree, &current_tree);
            json!({
                "snapshot_name": name,
                "added": diff["added"],
                "removed": diff["removed"],
                "modified": diff["modified"],
                "added_count": diff["added_count"],
                "removed_count": diff["removed_count"],
                "modified_count": diff["modified_count"]
            })
            .to_string()
        }
        Err(e) => error_response("ipc_error", format!("Failed to get current UI tree: {}", e)),
    }
}
