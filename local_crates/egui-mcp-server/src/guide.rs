//! Setup guide for egui-mcp
//!
//! This module contains the guide text displayed by `egui-mcp-server guide`.

/// Print the setup guide to stdout
pub fn print_guide() {
    let version = env!("CARGO_PKG_VERSION");
    print!(
        r#"
================================================================================
                        egui-mcp-server Setup Guide
                              Version {version}
================================================================================

This guide explains how to set up egui-mcp for UI automation with MCP clients
like Claude Code.

--------------------------------------------------------------------------------
STEP 1: Add egui-mcp-client to your egui application
--------------------------------------------------------------------------------

Add the dependency to your Cargo.toml:

    [dependencies]
    egui-mcp-client = "{version}"

Initialize the client in your app:

    use egui_mcp_client::McpClient;

    struct MyApp {{
        mcp_client: McpClient,
        // ... your other fields
    }}

    impl MyApp {{
        fn new(cc: &eframe::CreationContext<'_>) -> Self {{
            Self {{
                mcp_client: McpClient::new(cc),
                // ...
            }}
        }}
    }}

    impl eframe::App for MyApp {{
        fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {{
            // Call this at the end of your update function
            self.mcp_client.update(ctx, frame);

            // ... your UI code
        }}
    }}

--------------------------------------------------------------------------------
STEP 2: Configure MCP client (e.g., Claude Code)
--------------------------------------------------------------------------------

Create or edit `.mcp.json` in your project root:

    {{
      "mcpServers": {{
        "egui-mcp": {{
          "command": "egui-mcp-server",
          "args": [],
          "env": {{
            "XDG_RUNTIME_DIR": "/mnt/wslg/runtime-dir",
            "EGUI_MCP_APP_NAME": "your-app-name"
          }}
        }}
      }}
    }}

Replace "your-app-name" with your egui application's window title.

For cargo-based development, use:

    {{
      "mcpServers": {{
        "egui-mcp": {{
          "command": "cargo",
          "args": ["run", "-p", "egui-mcp-server"],
          "env": {{
            "XDG_RUNTIME_DIR": "/mnt/wslg/runtime-dir",
            "EGUI_MCP_APP_NAME": "your-app-name"
          }}
        }}
      }}
    }}

--------------------------------------------------------------------------------
STEP 3: Run your application
--------------------------------------------------------------------------------

1. Start your egui application (with egui-mcp-client integrated)
2. The MCP server will automatically connect when Claude Code starts
3. Use natural language to interact with your UI:
   - "Click the Submit button"
   - "Type 'hello' in the text field"
   - "Take a screenshot"
   - "Find all buttons on the screen"

--------------------------------------------------------------------------------
ENVIRONMENT VARIABLES
--------------------------------------------------------------------------------

  EGUI_MCP_APP_NAME    (Required) Target application's window title
  XDG_RUNTIME_DIR      Runtime directory for IPC socket (WSL: /mnt/wslg/runtime-dir)
  RUST_LOG             Log level (e.g., "info", "debug")

--------------------------------------------------------------------------------
AVAILABLE MCP TOOLS
--------------------------------------------------------------------------------

UI Tree & Search:
  - get_ui_tree       Get the full accessibility tree
  - find_by_label     Search elements by label (substring match)
  - find_by_role      Search elements by role (Button, TextInput, etc.)
  - get_element       Get detailed info about a specific element

Interaction:
  - click_element     Click an element by ID
  - click_at          Click at specific coordinates
  - set_text          Set text in a text input
  - keyboard_input    Send keyboard input
  - scroll            Scroll at a position
  - hover             Move mouse to position
  - drag              Drag from one position to another

Screenshots:
  - take_screenshot   Capture the application window
  - compare_screenshots  Compare two screenshots for similarity
  - diff_screenshots     Generate visual diff between screenshots

For more information, visit: https://github.com/dijdzv/egui-mcp

================================================================================
"#
    );
}
