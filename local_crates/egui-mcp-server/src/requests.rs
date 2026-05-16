//! MCP tool request types
//!
//! This module contains all request types used by MCP tool handlers.

use rmcp::schemars;
use serde::Deserialize;

/// Request for find_by_label tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindByLabelRequest {
    #[schemars(description = "Pattern to match against labels (substring match)")]
    pub pattern: String,
}

/// Request for find_by_label_exact tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindByLabelExactRequest {
    #[schemars(description = "Exact label text to match")]
    pub pattern: String,
}

/// Request for find_by_role tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindByRoleRequest {
    #[schemars(
        description = "Role to search for (e.g., 'Button', 'TextInput', 'CheckBox', 'Label')"
    )]
    pub role: String,
}

/// Request for get_element tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetElementRequest {
    #[schemars(description = "Node ID to retrieve (as string)")]
    pub id: String,
}

/// Request for click_element tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClickElementRequest {
    #[schemars(description = "Node ID of the element to click (as string)")]
    pub id: String,
}

/// Request for set_text tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetTextRequest {
    #[schemars(description = "Node ID of the text input element (as string)")]
    pub id: String,
    #[schemars(description = "Text content to set")]
    pub text: String,
}

/// Request for click_at tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClickAtRequest {
    #[schemars(description = "X coordinate")]
    pub x: f32,
    #[schemars(description = "Y coordinate")]
    pub y: f32,
    #[schemars(description = "Mouse button: 'left', 'right', or 'middle' (default: 'left')")]
    pub button: Option<String>,
}

/// Request for take_screenshot tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TakeScreenshotRequest {
    #[schemars(
        description = "If true, save screenshot to a temp file and return the path. If false (default), return base64-encoded data."
    )]
    pub save_to_file: Option<bool>,
}

/// Request for keyboard_input tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct KeyboardInputRequest {
    #[schemars(description = "Key to send (e.g., 'a', 'Enter', 'Escape', 'Tab')")]
    pub key: String,
}

/// Request for scroll tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScrollRequest {
    #[schemars(description = "X coordinate where to scroll")]
    pub x: f32,
    #[schemars(description = "Y coordinate where to scroll")]
    pub y: f32,
    #[schemars(description = "Horizontal scroll delta (positive = right)")]
    pub delta_x: Option<f32>,
    #[schemars(description = "Vertical scroll delta (positive = down)")]
    pub delta_y: Option<f32>,
}

/// Request for hover tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HoverRequest {
    #[schemars(description = "X coordinate to move mouse to")]
    pub x: f32,
    #[schemars(description = "Y coordinate to move mouse to")]
    pub y: f32,
}

/// Request for drag tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DragRequest {
    #[schemars(description = "Starting X coordinate")]
    pub start_x: f32,
    #[schemars(description = "Starting Y coordinate")]
    pub start_y: f32,
    #[schemars(description = "Ending X coordinate")]
    pub end_x: f32,
    #[schemars(description = "Ending Y coordinate")]
    pub end_y: f32,
    #[schemars(description = "Mouse button: 'left', 'right', or 'middle' (default: 'left')")]
    pub button: Option<String>,
}

/// Request for double_click tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DoubleClickRequest {
    #[schemars(description = "X coordinate")]
    pub x: f32,
    #[schemars(description = "Y coordinate")]
    pub y: f32,
    #[schemars(description = "Mouse button: 'left', 'right', or 'middle' (default: 'left')")]
    pub button: Option<String>,
}

/// Request for drag_element tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DragElementRequest {
    #[schemars(description = "Node ID of the element to drag (as string)")]
    pub source_id: String,
    #[schemars(description = "Ending X coordinate")]
    pub end_x: f32,
    #[schemars(description = "Ending Y coordinate")]
    pub end_y: f32,
    #[schemars(description = "Mouse button: 'left', 'right', or 'middle' (default: 'left')")]
    pub button: Option<String>,
}

/// Request for get_bounds tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetBoundsRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
}

/// Request for focus_element tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FocusElementRequest {
    #[schemars(description = "Node ID of the element to focus (as string)")]
    pub id: String,
}

/// Request for scroll_to_element tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScrollToElementRequest {
    #[schemars(description = "Node ID of the element to scroll into view (as string)")]
    pub id: String,
}

/// Request for get_value tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetValueRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
}

/// Request for set_value tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetValueRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
    #[schemars(description = "Value to set (number)")]
    pub value: f64,
}

/// Request for select_item tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SelectItemRequest {
    #[schemars(description = "Node ID of the container element (as string)")]
    pub id: String,
    #[schemars(description = "Index of the item to select (0-based)")]
    pub index: i32,
}

/// Request for deselect_item tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeselectItemRequest {
    #[schemars(description = "Node ID of the container element (as string)")]
    pub id: String,
    #[schemars(description = "Index of the item to deselect (0-based)")]
    pub index: i32,
}

/// Request for get_selected_count tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSelectedCountRequest {
    #[schemars(description = "Node ID of the container element (as string)")]
    pub id: String,
}

/// Request for select_all/clear_selection tools
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SelectionContainerRequest {
    #[schemars(description = "Node ID of the container element (as string)")]
    pub id: String,
}

/// Request for get_text tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTextRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
}

/// Request for get_text_selection tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTextSelectionRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
}

/// Request for set_text_selection tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetTextSelectionRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
    #[schemars(description = "Start offset of the selection")]
    pub start: i32,
    #[schemars(description = "End offset of the selection")]
    pub end: i32,
}

/// Request for get_caret_position tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCaretPositionRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
}

/// Request for set_caret_position tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetCaretPositionRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
    #[schemars(description = "Offset position for the caret")]
    pub offset: i32,
}

/// Request for state check tools (is_visible, is_enabled, is_focused, is_checked)
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ElementIdOnlyRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
}

/// Request for screenshot_element tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenshotElementRequest {
    #[schemars(description = "Node ID of the element to screenshot (as string)")]
    pub id: String,
    #[schemars(
        description = "If true, save screenshot to a temp file and return the path. If false (default), return base64-encoded data."
    )]
    pub save_to_file: Option<bool>,
}

/// Request for screenshot_region tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenshotRegionRequest {
    #[schemars(description = "X coordinate of the region")]
    pub x: f32,
    #[schemars(description = "Y coordinate of the region")]
    pub y: f32,
    #[schemars(description = "Width of the region")]
    pub width: f32,
    #[schemars(description = "Height of the region")]
    pub height: f32,
    #[schemars(
        description = "If true, save screenshot to a temp file and return the path. If false (default), return base64-encoded data."
    )]
    pub save_to_file: Option<bool>,
}

/// Request for wait_for_element tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitForElementRequest {
    #[schemars(description = "Label pattern to match (substring match)")]
    pub pattern: String,
    #[schemars(
        description = "If true (default), wait for element to appear. If false, wait for element to disappear."
    )]
    pub appear: Option<bool>,
    #[schemars(description = "Timeout in milliseconds (default: 5000)")]
    pub timeout_ms: Option<u64>,
}

/// Request for wait_for_state tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitForStateRequest {
    #[schemars(description = "Node ID of the element (as string)")]
    pub id: String,
    #[schemars(description = "State to wait for: 'visible', 'enabled', 'focused', or 'checked'")]
    pub state: String,
    #[schemars(description = "Expected state value (default: true)")]
    pub expected: Option<bool>,
    #[schemars(description = "Timeout in milliseconds (default: 5000)")]
    pub timeout_ms: Option<u64>,
}

/// Request for compare_screenshots tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompareScreenshotsRequest {
    #[schemars(description = "First screenshot as base64-encoded PNG")]
    pub base64_a: Option<String>,
    #[schemars(description = "Second screenshot as base64-encoded PNG")]
    pub base64_b: Option<String>,
    #[schemars(description = "Path to first screenshot file (alternative to base64_a)")]
    pub path_a: Option<String>,
    #[schemars(description = "Path to second screenshot file (alternative to base64_b)")]
    pub path_b: Option<String>,
    #[schemars(
        description = "Comparison algorithm: 'hybrid' (default), 'mssim' (structural), 'rms' (pixel-wise)"
    )]
    pub algorithm: Option<String>,
}

/// Request for diff_screenshots tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DiffScreenshotsRequest {
    #[schemars(description = "First screenshot as base64-encoded PNG")]
    pub base64_a: Option<String>,
    #[schemars(description = "Second screenshot as base64-encoded PNG")]
    pub base64_b: Option<String>,
    #[schemars(description = "Path to first screenshot file (alternative to base64_a)")]
    pub path_a: Option<String>,
    #[schemars(description = "Path to second screenshot file (alternative to base64_b)")]
    pub path_b: Option<String>,
    #[schemars(
        description = "If true, save diff image to a temp file and return the path. If false (default), return base64-encoded data."
    )]
    pub save_to_file: Option<bool>,
}

/// Request for highlight_element tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HighlightElementRequest {
    #[schemars(description = "Node ID of the element to highlight (as string)")]
    pub id: String,
    #[schemars(
        description = "Highlight color as hex string (e.g., '#ff0000' or '#ff000080' with alpha). Default: red"
    )]
    pub color: Option<String>,
    #[schemars(
        description = "Duration in milliseconds. 0 = highlight until cleared. Default: 3000"
    )]
    pub duration_ms: Option<u64>,
}

/// Request for save_snapshot tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveSnapshotRequest {
    #[schemars(description = "Name to identify this snapshot")]
    pub name: String,
}

/// Request for load_snapshot tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LoadSnapshotRequest {
    #[schemars(description = "Name of the snapshot to load")]
    pub name: String,
}

/// Request for diff_snapshots tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DiffSnapshotsRequest {
    #[schemars(description = "Name of the first snapshot")]
    pub name_a: String,
    #[schemars(description = "Name of the second snapshot")]
    pub name_b: String,
}

/// Request for diff_current tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DiffCurrentRequest {
    #[schemars(description = "Name of the snapshot to compare with current state")]
    pub name: String,
}

/// Request for get_logs tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetLogsRequest {
    #[schemars(
        description = "Minimum log level to return: 'TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR'. If omitted, returns all levels."
    )]
    pub level: Option<String>,
    #[schemars(description = "Maximum number of entries to return (default: all)")]
    pub limit: Option<usize>,
}

/// Request for start_perf_recording tool
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StartPerfRecordingRequest {
    #[schemars(
        description = "Duration to record in milliseconds. 0 = until get_perf_report is called (default: 0)"
    )]
    pub duration_ms: Option<u64>,
}
