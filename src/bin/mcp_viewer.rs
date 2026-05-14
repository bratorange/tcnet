// mcp_viewer: MCP stdio server that exposes TCNet controller state via MCP tools.
// Requires --features mcp

use clap::Parser;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use tcnet::{ApplicationConfig, NodeType, TCNetClient};
use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, Content, ListToolsResult,
        PaginatedRequestParams, ServerInfo, Tool,
        ServerCapabilities, Implementation,
    },
    service::{RequestContext, RoleServer},
    ErrorData as McpError,
    ServiceExt,
};
use serde_json::json;

#[derive(Parser)]
#[command(name = "mcp_viewer", about = "TCNet MCP server — exposes controller state as MCP tools")]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    bind_ip: Ipv4Addr,
}

#[derive(Debug, Default, Clone)]
struct ViewerState {
    is_connected: bool,
    node_count: usize,
    layers: Vec<LayerInfo>,
    mixer: MixerInfo,
}

#[derive(Debug, Default, Clone)]
struct LayerInfo {
    pub layer_index: usize,
    pub title: String,
    pub artist: String,
    pub bpm: f32,
    pub position_ms: u32,
    pub track_length_ms: u32,
    pub state_str: String,
}

#[derive(Debug, Default, Clone)]
struct MixerInfo {
    pub name: String,
    pub master_fader: u8,
    pub crossfader: u8,
}

struct ViewerMcpServer {
    state: Arc<Mutex<ViewerState>>,
}

impl ViewerMcpServer {
    fn make_tools() -> Vec<Tool> {
        let empty_schema: Arc<rmcp::model::JsonObject> = Arc::new(
            serde_json::from_value(json!({ "type": "object", "properties": {}, "required": [] })).unwrap()
        );
        vec![
            Tool::new("is_connected", "Check if a TCNet controller is connected on the network", Arc::clone(&empty_schema)),
            Tool::new("get_controller_state", "Get the current state of the TCNet DJ controller (layers and mixer)", Arc::clone(&empty_schema)),
        ]
    }
}

impl ServerHandler for ViewerMcpServer {
    fn get_info(&self) -> ServerInfo {
        let caps = ServerCapabilities::builder()
            .enable_tools()
            .build();
        ServerInfo::new(caps).with_server_info(Implementation::new("tcnet-viewer", "0.1.0"))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(Self::make_tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        match request.name.as_ref() {
            "is_connected" => {
                let state = self.state.lock().map_err(|_| McpError::internal_error("lock poisoned", None))?;
                let result = json!({
                    "connected": state.is_connected,
                    "node_count": state.node_count,
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }

            "get_controller_state" => {
                let state = self.state.lock().map_err(|_| McpError::internal_error("lock poisoned", None))?;
                if !state.is_connected {
                    return Ok(CallToolResult::error(vec![Content::text("No TCNet controller connected")]));
                }
                let layers: Vec<_> = state.layers.iter().map(|l| json!({
                    "layer": l.layer_index,
                    "title": l.title,
                    "artist": l.artist,
                    "bpm": l.bpm,
                    "position_ms": l.position_ms,
                    "track_length_ms": l.track_length_ms,
                    "state": l.state_str,
                })).collect();
                let result = json!({
                    "layers": layers,
                    "mixer": {
                        "name": state.mixer.name,
                        "master_fader": state.mixer.master_fader,
                        "crossfader": state.mixer.crossfader,
                    }
                });
                Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
            }

            _ => Err(McpError::method_not_found::<rmcp::model::CallToolRequestMethod>()),
        }
    }
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut node_config = ApplicationConfig::default();
    node_config.node_type = NodeType::Slave;
    node_config.address.set_ip(args.bind_ip);

    let mut client = TCNetClient::new(node_config);
    let state: Arc<Mutex<ViewerState>> = Arc::new(Mutex::new(ViewerState::default()));
    let state_for_mcp = Arc::clone(&state);

    // Start MCP server on a background thread (stdio transport)
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async move {
            let server = ViewerMcpServer { state: state_for_mcp };
            let transport = rmcp::transport::io::stdio();
            match server.serve(transport).await {
                Ok(running) => {
                    let _ = running.waiting().await;
                }
                Err(e) => {
                    eprintln!("MCP server error: {e}");
                }
            }
        });
    });

    // Main thread: poll TCNet state every 150ms
    loop {
        let nodes = client.active_nodes();
        let is_connected = !nodes.is_empty();
        let node_count = nodes.len();

        let controller_data: Option<(Vec<LayerInfo>, MixerInfo)> = if is_connected {
            if let Some(mut view) = client.get_any_controller_view() {
                let layers_snap = view.get_layers().to_vec();
                let mixer_snap = view.get_mixer().clone();

                let layers = layers_snap.iter().enumerate().map(|(i, l)| LayerInfo {
                    layer_index: i,
                    title: l.title.clone(),
                    artist: l.artist.clone(),
                    bpm: l.bpm.0 as f32 / 100.0,
                    position_ms: l.position_ms,
                    track_length_ms: l.track_length_ms,
                    state_str: format!("{:?}", l.state),
                }).collect();

                let mixer = MixerInfo {
                    name: mixer_snap.mixer_name.clone(),
                    master_fader: mixer_snap.master_fader_level,
                    crossfader: mixer_snap.crossfader,
                };

                Some((layers, mixer))
            } else {
                None
            }
        } else {
            None
        };

        if let Ok(mut s) = state.lock() {
            s.is_connected = is_connected;
            s.node_count = node_count;
            if let Some((layers, mixer)) = controller_data {
                s.layers = layers;
                s.mixer = mixer;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}
