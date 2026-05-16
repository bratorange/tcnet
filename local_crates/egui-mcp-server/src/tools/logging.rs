//! Logging tool implementations

use super::{ToolResult, error_response, success_response};
use crate::ipc_client::IpcClient;
use serde_json::json;

/// Get recent log entries from the egui application
pub async fn get_logs(
    ipc_client: &IpcClient,
    level: Option<&str>,
    limit: Option<usize>,
) -> ToolResult {
    match ipc_client
        .get_logs(level.map(|s| s.to_string()), limit)
        .await
    {
        Ok(logs) => json!({
            "count": logs.len(),
            "logs": logs
        })
        .to_string(),
        Err(e) => error_response("ipc_error", format!("Failed to get logs: {}", e)),
    }
}

/// Clear the log buffer in the egui application
pub async fn clear_logs(ipc_client: &IpcClient) -> ToolResult {
    match ipc_client.clear_logs().await {
        Ok(()) => success_response("Log buffer cleared"),
        Err(e) => error_response("ipc_error", format!("Failed to clear logs: {}", e)),
    }
}
