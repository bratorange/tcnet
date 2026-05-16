//! Basic tool implementations (ping, check_connection)

use crate::ipc_client::IpcClient;
use serde_json::json;

/// Ping the server to check if it's running
pub fn ping() -> String {
    "pong".to_string()
}

/// Check connection to the egui application
pub async fn check_connection(ipc_client: &IpcClient) -> String {
    if !ipc_client.is_socket_available() {
        return json!({
            "connected": false,
            "message": "No egui application socket found. Make sure the egui app is running with egui-mcp-client."
        })
        .to_string();
    }

    match ipc_client.ping().await {
        Ok(true) => json!({
            "connected": true,
            "message": "egui application is connected and responding"
        })
        .to_string(),
        Ok(false) => json!({
            "connected": false,
            "message": "egui application did not respond correctly"
        })
        .to_string(),
        Err(e) => json!({
            "connected": false,
            "message": format!("Failed to connect: {}", e)
        })
        .to_string(),
    }
}
