//! MCP server for egui UI automation
//!
//! This server provides MCP tools for interacting with egui applications.
//! Architecture:
//! - AT-SPI (Linux accessibility): UI tree, element search, clicks, text input
//! - IPC (direct client): Screenshots, coordinate-based input, keyboard, scroll

mod constants;
mod guide;
mod ipc_client;
mod requests;
mod tools;
mod utils;

#[cfg(target_os = "linux")]
mod atspi_client;
#[cfg(target_os = "linux")]
mod errors;

use anyhow::Result;
use clap::{Parser, Subcommand};
use ipc_client::IpcClient;
use requests::*;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use std::sync::Arc;
use tools::snapshot::SnapshotStore;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// MCP server for egui UI automation
#[derive(Parser)]
#[command(name = "egui-mcp-server")]
#[command(version, about, long_about = None)]
#[command(subcommand_required = true, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run as MCP server
    Serve,
    /// Show setup guide for MCP client and egui app integration
    Guide,
}

/// egui-mcp server handler
#[derive(Clone)]
struct EguiMcpServer {
    tool_router: ToolRouter<Self>,
    ipc_client: Arc<IpcClient>,
    snapshots: SnapshotStore,
    app_name: String,
}

impl EguiMcpServer {
    fn new(app_name: String) -> Self {
        let tool_router = Self::tool_router();
        let ipc_client = Arc::new(IpcClient::new());
        let snapshots = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        Self {
            tool_router,
            ipc_client,
            snapshots,
            app_name,
        }
    }
}

#[tool_router]
impl EguiMcpServer {
    // ========================================================================
    // Basic tools
    // ========================================================================

    #[tool(description = "Ping the egui-mcp server to verify it's running")]
    async fn ping(&self) -> String {
        tools::basic::ping()
    }

    #[tool(description = "Check if the egui application is connected and responding")]
    async fn check_connection(&self) -> String {
        tools::basic::check_connection(&self.ipc_client).await
    }

    // ========================================================================
    // UI Tree tools (AT-SPI)
    // ========================================================================

    #[tool(description = "Get the full UI tree from the egui application as JSON")]
    async fn get_ui_tree(&self) -> String {
        tools::tree::get_ui_tree(&self.app_name, &self.ipc_client).await
    }

    #[tool(description = "Find UI elements by their label text (substring match)")]
    async fn find_by_label(
        &self,
        Parameters(FindByLabelRequest { pattern }): Parameters<FindByLabelRequest>,
    ) -> String {
        tools::tree::find_by_label(&self.app_name, &self.ipc_client, &pattern, false).await
    }

    #[tool(description = "Find UI elements by their label text (exact match)")]
    async fn find_by_label_exact(
        &self,
        Parameters(FindByLabelExactRequest { pattern }): Parameters<FindByLabelExactRequest>,
    ) -> String {
        tools::tree::find_by_label(&self.app_name, &self.ipc_client, &pattern, true).await
    }

    #[tool(
        description = "Find UI elements by their role (e.g., 'Button', 'TextInput', 'CheckBox', 'Label')"
    )]
    async fn find_by_role(
        &self,
        Parameters(FindByRoleRequest { role }): Parameters<FindByRoleRequest>,
    ) -> String {
        tools::tree::find_by_role(&self.app_name, &self.ipc_client, &role).await
    }

    #[tool(
        description = "Get detailed information about a specific UI element by its ID (as string)"
    )]
    async fn get_element(
        &self,
        Parameters(GetElementRequest { id }): Parameters<GetElementRequest>,
    ) -> String {
        tools::tree::get_element(&self.app_name, &self.ipc_client, &id).await
    }

    // ========================================================================
    // Element action tools (AT-SPI)
    // ========================================================================

    #[tool(description = "Click a UI element by its ID (as string). Uses AT-SPI on Linux, IPC bounds-based click on macOS/other.")]
    async fn click_element(
        &self,
        Parameters(ClickElementRequest { id }): Parameters<ClickElementRequest>,
    ) -> String {
        tools::action::click_element(&self.app_name, &self.ipc_client, &id).await
    }

    #[tool(
        description = "Set text content of a text input element by its ID (as string). Note: Does not work with egui (AccessKit limitation). Use keyboard_input instead. Uses AT-SPI EditableText interface."
    )]
    async fn set_text(
        &self,
        Parameters(SetTextRequest { id, text }): Parameters<SetTextRequest>,
    ) -> String {
        tools::action::set_text(&self.app_name, &id, &text).await
    }

    #[tool(
        description = "Drag a UI element to a target position. Gets element center via AT-SPI and drags to target coordinates via IPC."
    )]
    async fn drag_element(
        &self,
        Parameters(DragElementRequest {
            source_id,
            end_x,
            end_y,
            button,
        }): Parameters<DragElementRequest>,
    ) -> String {
        tools::action::drag_element(
            &self.app_name,
            &self.ipc_client,
            &source_id,
            end_x,
            end_y,
            button.as_deref(),
        )
        .await
    }

    // ========================================================================
    // Component interface tools (AT-SPI)
    // ========================================================================

    #[tool(
        description = "Get the bounding box (position and size) of a UI element by ID. Uses AT-SPI Component interface."
    )]
    async fn get_bounds(
        &self,
        Parameters(GetBoundsRequest { id }): Parameters<GetBoundsRequest>,
    ) -> String {
        tools::component::get_bounds(&self.app_name, &id).await
    }

    #[tool(description = "Focus a UI element by ID. Uses AT-SPI Component interface.")]
    async fn focus_element(
        &self,
        Parameters(FocusElementRequest { id }): Parameters<FocusElementRequest>,
    ) -> String {
        tools::component::focus_element(&self.app_name, &id).await
    }

    #[tool(description = "Scroll a UI element into view by ID. Uses AT-SPI Component interface.")]
    async fn scroll_to_element(
        &self,
        Parameters(ScrollToElementRequest { id }): Parameters<ScrollToElementRequest>,
    ) -> String {
        tools::component::scroll_to_element(&self.app_name, &id).await
    }

    // ========================================================================
    // Text interface tools (AT-SPI)
    // ========================================================================

    #[tool(
        description = "Get the text content, length, and caret position of a text element. Uses AT-SPI Text interface."
    )]
    async fn get_text(
        &self,
        Parameters(GetTextRequest { id }): Parameters<GetTextRequest>,
    ) -> String {
        tools::text::get_text(&self.app_name, &id).await
    }

    #[tool(
        description = "Get the current text selection range (start and end offsets). Returns -1 if element has no focus. Uses AT-SPI Text interface."
    )]
    async fn get_text_selection(
        &self,
        Parameters(GetTextSelectionRequest { id }): Parameters<GetTextSelectionRequest>,
    ) -> String {
        tools::text::get_text_selection(&self.app_name, &id).await
    }

    #[tool(
        description = "Set the text selection range (start and end offsets). Requires focus first (use focus_element). Uses AT-SPI Text interface."
    )]
    async fn set_text_selection(
        &self,
        Parameters(SetTextSelectionRequest { id, start, end }): Parameters<SetTextSelectionRequest>,
    ) -> String {
        tools::text::set_text_selection(&self.app_name, &id, start, end).await
    }

    #[tool(
        description = "Get the caret (cursor) position in a text element. Returns -1 if element has no focus. Uses AT-SPI Text interface."
    )]
    async fn get_caret_position(
        &self,
        Parameters(GetCaretPositionRequest { id }): Parameters<GetCaretPositionRequest>,
    ) -> String {
        tools::text::get_caret_position(&self.app_name, &id).await
    }

    #[tool(
        description = "Set the caret (cursor) position in a text element. Requires focus first (use focus_element). Uses AT-SPI Text interface."
    )]
    async fn set_caret_position(
        &self,
        Parameters(SetCaretPositionRequest { id, offset }): Parameters<SetCaretPositionRequest>,
    ) -> String {
        tools::text::set_caret_position(&self.app_name, &id, offset).await
    }

    // ========================================================================
    // Value interface tools (AT-SPI)
    // ========================================================================

    #[tool(
        description = "Get the current value, min, max, and increment of a value element (slider, progress bar, etc.). Uses AT-SPI Value interface."
    )]
    async fn get_value(
        &self,
        Parameters(GetValueRequest { id }): Parameters<GetValueRequest>,
    ) -> String {
        tools::value::get_value(&self.app_name, &id).await
    }

    #[tool(
        description = "Set the value of a value element (slider, etc.). Uses AT-SPI Value interface."
    )]
    async fn set_value(
        &self,
        Parameters(SetValueRequest { id, value }): Parameters<SetValueRequest>,
    ) -> String {
        tools::value::set_value(&self.app_name, &id, value).await
    }

    // ========================================================================
    // State interface tools (AT-SPI)
    // ========================================================================

    #[tool(description = "Check if a UI element is visible. Uses AT-SPI State interface.")]
    async fn is_visible(
        &self,
        Parameters(ElementIdOnlyRequest { id }): Parameters<ElementIdOnlyRequest>,
    ) -> String {
        tools::state::is_visible(&self.app_name, &id).await
    }

    #[tool(description = "Check if a UI element is enabled. Uses AT-SPI State interface.")]
    async fn is_enabled(
        &self,
        Parameters(ElementIdOnlyRequest { id }): Parameters<ElementIdOnlyRequest>,
    ) -> String {
        tools::state::is_enabled(&self.app_name, &id).await
    }

    #[tool(description = "Check if a UI element is focused. Uses AT-SPI State interface.")]
    async fn is_focused(
        &self,
        Parameters(ElementIdOnlyRequest { id }): Parameters<ElementIdOnlyRequest>,
    ) -> String {
        tools::state::is_focused(&self.app_name, &id).await
    }

    #[tool(
        description = "Check if a UI element is checked or pressed (for checkboxes, toggle buttons). Returns checked: true/false for checkable elements, or checked: null for non-checkable elements. Uses AT-SPI State interface."
    )]
    async fn is_checked(
        &self,
        Parameters(ElementIdOnlyRequest { id }): Parameters<ElementIdOnlyRequest>,
    ) -> String {
        tools::state::is_checked(&self.app_name, &id).await
    }

    // ========================================================================
    // Selection interface tools (AT-SPI)
    // ========================================================================

    #[tool(
        description = "Select an item by index in a selection container (list, combo box, etc.). Note: Does not work with egui ComboBox (items not exposed as children). Use click_at + keyboard_input instead. Uses AT-SPI Selection interface."
    )]
    async fn select_item(
        &self,
        Parameters(SelectItemRequest { id, index }): Parameters<SelectItemRequest>,
    ) -> String {
        tools::selection::select_item(&self.app_name, &id, index).await
    }

    #[tool(
        description = "Deselect an item by index in a selection container. Note: Does not work with egui ComboBox. Use click_at + keyboard_input instead. Uses AT-SPI Selection interface."
    )]
    async fn deselect_item(
        &self,
        Parameters(DeselectItemRequest { id, index }): Parameters<DeselectItemRequest>,
    ) -> String {
        tools::selection::deselect_item(&self.app_name, &id, index).await
    }

    #[tool(
        description = "Get the number of selected items in a selection container. For egui ComboBox, checks name property (returns 0 or 1). Uses AT-SPI Selection interface."
    )]
    async fn get_selected_count(
        &self,
        Parameters(GetSelectedCountRequest { id }): Parameters<GetSelectedCountRequest>,
    ) -> String {
        tools::selection::get_selected_count(&self.app_name, &id).await
    }

    #[tool(
        description = "Select all items in a selection container. Note: Not useful for egui (only has single-selection widgets like ComboBox and RadioGroup). Uses AT-SPI Selection interface."
    )]
    async fn select_all(
        &self,
        Parameters(SelectionContainerRequest { id }): Parameters<SelectionContainerRequest>,
    ) -> String {
        tools::selection::select_all(&self.app_name, &id).await
    }

    #[tool(
        description = "Clear all selections in a selection container. Note: Not useful for egui (only has single-selection widgets like ComboBox and RadioGroup). Uses AT-SPI Selection interface."
    )]
    async fn clear_selection(
        &self,
        Parameters(SelectionContainerRequest { id }): Parameters<SelectionContainerRequest>,
    ) -> String {
        tools::selection::clear_selection(&self.app_name, &id).await
    }

    // ========================================================================
    // IPC input tools
    // ========================================================================

    #[tool(description = "Click at specific coordinates in the egui application window")]
    async fn click_at(
        &self,
        Parameters(ClickAtRequest { x, y, button }): Parameters<ClickAtRequest>,
    ) -> String {
        tools::input::click_at(&self.ipc_client, x, y, button.as_deref()).await
    }

    #[tool(description = "Send keyboard input to the egui application")]
    async fn keyboard_input(
        &self,
        Parameters(KeyboardInputRequest { key }): Parameters<KeyboardInputRequest>,
    ) -> String {
        tools::input::keyboard_input(&self.ipc_client, &key).await
    }

    #[tool(description = "Scroll at specific coordinates in the egui application window")]
    async fn scroll(
        &self,
        Parameters(ScrollRequest {
            x,
            y,
            delta_x,
            delta_y,
        }): Parameters<ScrollRequest>,
    ) -> String {
        tools::input::scroll(&self.ipc_client, x, y, delta_x, delta_y).await
    }

    #[tool(
        description = "Move mouse to specific coordinates in the egui application window (hover)"
    )]
    async fn hover(&self, Parameters(HoverRequest { x, y }): Parameters<HoverRequest>) -> String {
        tools::input::hover(&self.ipc_client, x, y).await
    }

    #[tool(description = "Drag from one point to another in the egui application window")]
    async fn drag(
        &self,
        Parameters(DragRequest {
            start_x,
            start_y,
            end_x,
            end_y,
            button,
        }): Parameters<DragRequest>,
    ) -> String {
        tools::input::drag(
            &self.ipc_client,
            start_x,
            start_y,
            end_x,
            end_y,
            button.as_deref(),
        )
        .await
    }

    #[tool(description = "Double click at specific coordinates in the egui application window")]
    async fn double_click(
        &self,
        Parameters(DoubleClickRequest { x, y, button }): Parameters<DoubleClickRequest>,
    ) -> String {
        tools::input::double_click(&self.ipc_client, x, y, button.as_deref()).await
    }

    // ========================================================================
    // Screenshot tools (IPC + AT-SPI)
    // ========================================================================

    #[tool(
        description = "Take a screenshot of the egui application. Returns base64-encoded PNG image data."
    )]
    async fn take_screenshot(
        &self,
        Parameters(TakeScreenshotRequest { save_to_file }): Parameters<TakeScreenshotRequest>,
    ) -> Content {
        match tools::screenshot::take_screenshot(&self.ipc_client, save_to_file.unwrap_or(false))
            .await
        {
            Ok(content) => content,
            Err(error_json) => Content::text(error_json),
        }
    }

    #[tool(
        description = "Take a screenshot of a specific UI element by ID. Captures the full screen and crops to element bounds."
    )]
    async fn screenshot_element(
        &self,
        Parameters(ScreenshotElementRequest { id, save_to_file }): Parameters<
            ScreenshotElementRequest,
        >,
    ) -> Content {
        match tools::screenshot::screenshot_element(
            &self.app_name,
            &self.ipc_client,
            &id,
            save_to_file.unwrap_or(false),
        )
        .await
        {
            Ok(content) => content,
            Err(error_json) => Content::text(error_json),
        }
    }

    #[tool(
        description = "Take a screenshot of a specific region. Captures the full screen and crops to the specified coordinates."
    )]
    async fn screenshot_region(
        &self,
        Parameters(ScreenshotRegionRequest {
            x,
            y,
            width,
            height,
            save_to_file,
        }): Parameters<ScreenshotRegionRequest>,
    ) -> Content {
        match tools::screenshot::screenshot_region(
            &self.ipc_client,
            x,
            y,
            width,
            height,
            save_to_file.unwrap_or(false),
        )
        .await
        {
            Ok(content) => content,
            Err(error_json) => Content::text(error_json),
        }
    }

    #[tool(
        description = "Compare two screenshots and return similarity score. Returns a score between 0.0 (completely different) and 1.0 (identical)."
    )]
    async fn compare_screenshots(
        &self,
        Parameters(CompareScreenshotsRequest {
            base64_a,
            base64_b,
            path_a,
            path_b,
            algorithm,
        }): Parameters<CompareScreenshotsRequest>,
    ) -> String {
        tools::screenshot::compare_screenshots(
            base64_a.as_deref(),
            base64_b.as_deref(),
            path_a.as_deref(),
            path_b.as_deref(),
            algorithm.as_deref(),
        )
    }

    #[tool(
        description = "Generate a visual diff image highlighting differences between two screenshots. Returns the diff image as base64-encoded PNG."
    )]
    async fn diff_screenshots(
        &self,
        Parameters(DiffScreenshotsRequest {
            base64_a,
            base64_b,
            path_a,
            path_b,
            save_to_file,
        }): Parameters<DiffScreenshotsRequest>,
    ) -> Content {
        match tools::screenshot::diff_screenshots(
            base64_a.as_deref(),
            base64_b.as_deref(),
            path_a.as_deref(),
            path_b.as_deref(),
            save_to_file.unwrap_or(false),
        ) {
            Ok(content) => content,
            Err(error_json) => Content::text(error_json),
        }
    }

    // ========================================================================
    // Highlight tools (IPC + AT-SPI)
    // ========================================================================

    #[tool(
        description = "Draw highlight overlay on element by ID. Requires AT-SPI to get element bounds."
    )]
    async fn highlight_element(
        &self,
        Parameters(HighlightElementRequest {
            id,
            color,
            duration_ms,
        }): Parameters<HighlightElementRequest>,
    ) -> String {
        tools::highlight::highlight_element(
            &self.app_name,
            &self.ipc_client,
            &id,
            color.as_deref(),
            duration_ms,
        )
        .await
    }

    #[tool(description = "Remove all highlights")]
    async fn clear_highlights(&self) -> String {
        tools::highlight::clear_highlights(&self.ipc_client).await
    }

    // ========================================================================
    // Snapshot tools
    // ========================================================================

    #[tool(description = "Save current UI tree state as a named snapshot for later comparison")]
    async fn save_snapshot(&self, Parameters(req): Parameters<SaveSnapshotRequest>) -> String {
        tools::snapshot::save_snapshot(&self.app_name, &self.ipc_client, &self.snapshots, &req.name).await
    }

    #[tool(description = "Load a saved UI tree snapshot")]
    async fn load_snapshot(&self, Parameters(req): Parameters<LoadSnapshotRequest>) -> String {
        tools::snapshot::load_snapshot(&self.snapshots, &req.name)
    }

    #[tool(description = "Compare two saved snapshots and return the differences")]
    async fn diff_snapshots(&self, Parameters(req): Parameters<DiffSnapshotsRequest>) -> String {
        tools::snapshot::diff_snapshots(&self.snapshots, &req.name_a, &req.name_b)
    }

    #[tool(description = "Compare current UI tree state with a saved snapshot")]
    async fn diff_current(&self, Parameters(req): Parameters<DiffCurrentRequest>) -> String {
        tools::snapshot::diff_current(&self.app_name, &self.ipc_client, &self.snapshots, &req.name).await
    }

    // ========================================================================
    // Logging tools (IPC)
    // ========================================================================

    #[tool(
        description = "Get recent log entries from the egui application. Note: Requires the egui app to be configured with McpLogLayer."
    )]
    async fn get_logs(&self, Parameters(req): Parameters<GetLogsRequest>) -> String {
        tools::logging::get_logs(&self.ipc_client, req.level.as_deref(), req.limit).await
    }

    #[tool(description = "Clear the log buffer in the egui application")]
    async fn clear_logs(&self) -> String {
        tools::logging::clear_logs(&self.ipc_client).await
    }

    // ========================================================================
    // Performance tools (IPC)
    // ========================================================================

    #[tool(
        description = "Get current frame statistics (FPS, frame time) from the egui application. Note: Requires the egui app to call record_frame()."
    )]
    async fn get_frame_stats(&self) -> String {
        tools::perf::get_frame_stats(&self.ipc_client).await
    }

    #[tool(
        description = "Start recording performance data for later analysis. Call get_perf_report to stop and get results."
    )]
    async fn start_perf_recording(
        &self,
        Parameters(req): Parameters<StartPerfRecordingRequest>,
    ) -> String {
        tools::perf::start_perf_recording(&self.ipc_client, req.duration_ms).await
    }

    #[tool(
        description = "Stop performance recording and get the report with statistics including percentiles."
    )]
    async fn get_perf_report(&self) -> String {
        tools::perf::get_perf_report(&self.ipc_client).await
    }

    // ========================================================================
    // Wait tools (AT-SPI)
    // ========================================================================

    #[tool(
        description = "Wait for a UI element to appear or disappear. Polls every 100ms until the condition is met or timeout."
    )]
    async fn wait_for_element(
        &self,
        Parameters(WaitForElementRequest {
            pattern,
            appear,
            timeout_ms,
        }): Parameters<WaitForElementRequest>,
    ) -> String {
        tools::wait::wait_for_element(
            &self.app_name,
            &self.ipc_client,
            &pattern,
            appear.unwrap_or(true),
            timeout_ms.unwrap_or(tools::wait::DEFAULT_TIMEOUT_MS),
        )
        .await
    }

    #[tool(
        description = "Wait for a UI element's state to reach an expected value. Polls every 100ms until the condition is met or timeout. Supported states: 'visible', 'enabled', 'focused', 'checked'."
    )]
    async fn wait_for_state(
        &self,
        Parameters(WaitForStateRequest {
            id,
            state,
            expected,
            timeout_ms,
        }): Parameters<WaitForStateRequest>,
    ) -> String {
        tools::wait::wait_for_state(
            &self.app_name,
            &self.ipc_client,
            &id,
            &state,
            expected.unwrap_or(true),
            timeout_ms.unwrap_or(tools::wait::DEFAULT_TIMEOUT_MS),
        )
        .await
    }
}

#[tool_handler]
impl ServerHandler for EguiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "egui-mcp server provides tools for UI automation of egui applications. \
                 Use 'ping' to verify the server is running, 'check_connection' to verify \
                 the egui app is connected, 'get_ui_tree' to inspect the full UI structure, \
                 'find_by_label' for substring search, 'find_by_label_exact' for exact match, \
                 'find_by_role' to search by role (e.g., Button, TextInput), \
                 'get_element' to get details by ID (pass ID as string), \
                 'click_element' to click an element by ID (AT-SPI on Linux, IPC bounds-center on macOS), \
                 'set_text' to input text into a text field by ID (AT-SPI), \
                 'click_at' to click at specific coordinates (IPC), \
                 'double_click' to double click at specific coordinates (IPC), \
                 'keyboard_input' to send keyboard input (IPC), \
                 'scroll' to scroll at specific coordinates (IPC), \
                 'hover' to move mouse to specific coordinates (IPC), \
                 'drag' to drag from one point to another (IPC), \
                 'take_screenshot' to capture the current UI (IPC), \
                 'drag_element' to drag an element to target coordinates (AT-SPI + IPC), \
                 'get_bounds' to get element bounding box (AT-SPI Component), \
                 'focus_element' to focus an element (AT-SPI Component), \
                 'scroll_to_element' to scroll element into view (AT-SPI Component), \
                 'get_value' to get slider/progress value (AT-SPI Value), \
                 'set_value' to set slider value (AT-SPI Value), \
                 'select_item' to select item in list/combo (AT-SPI Selection), \
                 'deselect_item' to deselect item (AT-SPI Selection), \
                 'get_selected_count' to count selected items (AT-SPI Selection), \
                 'select_all' to select all items (AT-SPI Selection), \
                 'clear_selection' to clear all selections (AT-SPI Selection), \
                 'get_text' to get text content (AT-SPI Text), \
                 'get_text_selection' to get selected text range (AT-SPI Text), \
                 'set_text_selection' to select text range (AT-SPI Text), \
                 'get_caret_position' to get cursor position (AT-SPI Text), \
                 'set_caret_position' to set cursor position (AT-SPI Text), \
                 'is_visible' to check if element is visible (AT-SPI State), \
                 'is_enabled' to check if element is enabled (AT-SPI State), \
                 'is_focused' to check if element is focused (AT-SPI State), \
                 'is_checked' to check if element is checked/pressed (AT-SPI State), \
                 'screenshot_element' to capture a specific element (AT-SPI + IPC), \
                 'screenshot_region' to capture a specific region (IPC), \
                 'wait_for_element' to wait for element to appear/disappear (AT-SPI), \
                 'wait_for_state' to wait for element state change (AT-SPI), \
                 'compare_screenshots' to compare two screenshots and get similarity score, \
                 'diff_screenshots' to generate a visual diff image highlighting differences, \
                 'highlight_element' to draw a colored highlight on an element (AT-SPI + IPC), and \
                 'clear_highlights' to remove all highlights (IPC)."
                    .into(),
            ),
        }
    }
}

async fn run_server() -> Result<()> {
    // Initialize logging to stderr (stdout is used for MCP communication)
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    tracing::info!("Starting egui-mcp server...");

    // Enable session accessibility on Linux
    // This tells accessible applications (like egui with AccessKit) that an AT client is present
    #[cfg(target_os = "linux")]
    {
        match atspi::connection::set_session_accessibility(true).await {
            Ok(()) => tracing::info!("Session accessibility enabled"),
            Err(e) => tracing::warn!("Failed to enable session accessibility: {}", e),
        }
    }

    // Get application name from environment variable
    let app_name = std::env::var("EGUI_MCP_APP_NAME").map_err(|_| {
        anyhow::anyhow!(
            "EGUI_MCP_APP_NAME environment variable not set. \
             Please set it in .mcp.json env section. \
             Run 'egui-mcp-server guide' for setup instructions."
        )
    })?;

    tracing::info!("Target application: {}", app_name);

    // Create and run the server
    let server = EguiMcpServer::new(app_name);
    let service = server.serve(stdio()).await?;

    tracing::info!("Server started, waiting for connections...");
    service.waiting().await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve => run_server().await,
        Commands::Guide => {
            guide::print_guide();
            Ok(())
        }
    }
}
