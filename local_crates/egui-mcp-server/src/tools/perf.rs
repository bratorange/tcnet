//! Performance tool implementations

use super::{ToolResult, error_response, success_response};
use crate::ipc_client::IpcClient;
use serde_json::json;

/// Get current frame statistics
pub async fn get_frame_stats(ipc_client: &IpcClient) -> ToolResult {
    match ipc_client.get_frame_stats().await {
        Ok(stats) => json!({
            "fps": stats.fps,
            "frame_time_ms": stats.frame_time_ms,
            "sample_count": stats.sample_count
        })
        .to_string(),
        Err(e) => error_response("ipc_error", format!("Failed to get frame stats: {}", e)),
    }
}

/// Start recording performance data
pub async fn start_perf_recording(ipc_client: &IpcClient, duration_ms: Option<u64>) -> ToolResult {
    let duration = duration_ms.unwrap_or(0);
    match ipc_client.start_perf_recording(duration).await {
        Ok(()) => {
            if duration > 0 {
                success_response(format!("Recording started for {}ms", duration))
            } else {
                success_response("Recording started (call get_perf_report to stop)")
            }
        }
        Err(e) => error_response("ipc_error", format!("Failed to start recording: {}", e)),
    }
}

/// Get performance report (stops recording)
pub async fn get_perf_report(ipc_client: &IpcClient) -> ToolResult {
    match ipc_client.get_perf_report().await {
        Ok(Some(report)) => json!({
            "duration_ms": report.duration_ms,
            "total_frames": report.total_frames,
            "avg_fps": report.avg_fps,
            "avg_frame_time_ms": report.avg_frame_time_ms,
            "min_frame_time_ms": report.min_frame_time_ms,
            "max_frame_time_ms": report.max_frame_time_ms,
            "p95_frame_time_ms": report.p95_frame_time_ms,
            "p99_frame_time_ms": report.p99_frame_time_ms
        })
        .to_string(),
        Ok(None) => error_response(
            "no_data",
            "No performance recording active or no frames recorded",
        ),
        Err(e) => error_response(
            "ipc_error",
            format!("Failed to get performance report: {}", e),
        ),
    }
}
