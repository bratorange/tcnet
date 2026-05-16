use std::collections::VecDeque;

// Bridge between GUI thread and MCP server thread
#[derive(Debug, Default)]
pub struct SimBridge {
    pub commands: VecDeque<SimCmd>,
    pub deck1: DeckBridgeState,
    pub deck2: DeckBridgeState,
    pub crossfader: u8,
    pub available_tracks: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct DeckBridgeState {
    pub title: String,
    pub artist: String,
    pub bpm: f32,
    pub position_ms: u32,
    pub duration_ms: u32,
    pub is_playing: bool,
    pub is_loaded: bool,
}

#[derive(Debug)]
pub enum SimCmd {
    Play(usize),
    Pause(usize),
    Stop(usize),
    LoadTrack { deck: usize, filter: String },
    SetCrossfader(u8),
}

// MCP server implementation (requires "mcp" feature)
#[cfg(feature = "mcp")]
pub mod server {
    use std::sync::{Arc, Mutex};
    use rmcp::{
        ServerHandler,
        model::{
            CallToolRequestParams, CallToolResult, Content, ListToolsResult,
            PaginatedRequestParams, ServerInfo, Tool,
        },
        service::RequestContext,
        service::RoleServer,
        ErrorData as McpError,
    };
    use serde_json::json;
    use super::SimBridge;

    #[derive(Clone)]
    pub struct SimMcpServer {
        pub bridge: Arc<Mutex<SimBridge>>,
    }

    impl SimMcpServer {
        pub fn new(bridge: Arc<Mutex<SimBridge>>) -> Self {
            Self { bridge }
        }

        fn make_tools() -> Vec<Tool> {
            let empty_schema: Arc<rmcp::model::JsonObject> = Arc::new(
                serde_json::from_value(json!({ "type": "object", "properties": {}, "required": [] })).unwrap()
            );
            let deck_schema: Arc<rmcp::model::JsonObject> = Arc::new(
                serde_json::from_value(json!({
                    "type": "object",
                    "properties": { "deck": { "type": "integer", "description": "Deck number (1 or 2)" } },
                    "required": ["deck"]
                })).unwrap()
            );
            vec![
                Tool::new("get_deck_state", "Get the current state of both CDJ decks and crossfader", Arc::clone(&empty_schema)),
                Tool::new("play", "Start playback on a deck (1 or 2)", Arc::clone(&deck_schema)),
                Tool::new("pause", "Pause playback on a deck (1 or 2)", Arc::clone(&deck_schema)),
                Tool::new("stop", "Stop playback on a deck (1 or 2)", Arc::clone(&deck_schema)),
                Tool::new("load_track", "Load a track onto a deck by filter string", Arc::new(
                    serde_json::from_value(json!({
                        "type": "object",
                        "properties": {
                            "deck": { "type": "integer", "description": "Deck number (1 or 2)" },
                            "filter": { "type": "string", "description": "Search filter for track title/artist" }
                        },
                        "required": ["deck", "filter"]
                    })).unwrap()
                )),
                Tool::new("set_crossfader", "Set the crossfader position (0-255, 0=full left, 255=full right)", Arc::new(
                    serde_json::from_value(json!({
                        "type": "object",
                        "properties": { "value": { "type": "integer", "minimum": 0, "maximum": 255 } },
                        "required": ["value"]
                    })).unwrap()
                )),
                Tool::new("list_tracks", "List available tracks in the virtual USB library", Arc::clone(&empty_schema)),
            ]
        }
    }

    impl ServerHandler for SimMcpServer {
        fn get_info(&self) -> ServerInfo {
            use rmcp::model::{ServerCapabilities, Implementation};
            let caps = ServerCapabilities::builder()
                .enable_tools()
                .build();
            ServerInfo::new(caps).with_server_info(Implementation::new("tcnet-simulator", "0.1.0"))
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
            let args = request.arguments.unwrap_or_default();

            match request.name.as_ref() {
                "get_deck_state" => {
                    let bridge = self.bridge.lock().map_err(|_| {
                        McpError::internal_error("bridge lock poisoned", None)
                    })?;
                    let result = json!({
                        "deck1": {
                            "title": bridge.deck1.title,
                            "artist": bridge.deck1.artist,
                            "bpm": bridge.deck1.bpm,
                            "position_ms": bridge.deck1.position_ms,
                            "duration_ms": bridge.deck1.duration_ms,
                            "is_playing": bridge.deck1.is_playing,
                            "is_loaded": bridge.deck1.is_loaded,
                        },
                        "deck2": {
                            "title": bridge.deck2.title,
                            "artist": bridge.deck2.artist,
                            "bpm": bridge.deck2.bpm,
                            "position_ms": bridge.deck2.position_ms,
                            "duration_ms": bridge.deck2.duration_ms,
                            "is_playing": bridge.deck2.is_playing,
                            "is_loaded": bridge.deck2.is_loaded,
                        },
                        "crossfader": bridge.crossfader,
                    });
                    Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
                }

                "play" => {
                    let deck = args.get("deck")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| McpError::invalid_params("missing deck", None))? as usize;
                    if deck < 1 || deck > 2 {
                        return Err(McpError::invalid_params("deck must be 1 or 2", None));
                    }
                    self.bridge.lock().map_err(|_| McpError::internal_error("bridge lock poisoned", None))?
                        .commands.push_back(super::SimCmd::Play(deck));
                    Ok(CallToolResult::success(vec![Content::text(format!("Play command sent to deck {deck}"))]))
                }

                "pause" => {
                    let deck = args.get("deck")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| McpError::invalid_params("missing deck", None))? as usize;
                    if deck < 1 || deck > 2 {
                        return Err(McpError::invalid_params("deck must be 1 or 2", None));
                    }
                    self.bridge.lock().map_err(|_| McpError::internal_error("bridge lock poisoned", None))?
                        .commands.push_back(super::SimCmd::Pause(deck));
                    Ok(CallToolResult::success(vec![Content::text(format!("Pause command sent to deck {deck}"))]))
                }

                "stop" => {
                    let deck = args.get("deck")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| McpError::invalid_params("missing deck", None))? as usize;
                    if deck < 1 || deck > 2 {
                        return Err(McpError::invalid_params("deck must be 1 or 2", None));
                    }
                    self.bridge.lock().map_err(|_| McpError::internal_error("bridge lock poisoned", None))?
                        .commands.push_back(super::SimCmd::Stop(deck));
                    Ok(CallToolResult::success(vec![Content::text(format!("Stop command sent to deck {deck}"))]))
                }

                "load_track" => {
                    let deck = args.get("deck")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| McpError::invalid_params("missing deck", None))? as usize;
                    let filter = args.get("filter")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| McpError::invalid_params("missing filter", None))?
                        .to_string();
                    if deck < 1 || deck > 2 {
                        return Err(McpError::invalid_params("deck must be 1 or 2", None));
                    }
                    self.bridge.lock().map_err(|_| McpError::internal_error("bridge lock poisoned", None))?
                        .commands.push_back(super::SimCmd::LoadTrack { deck, filter: filter.clone() });
                    Ok(CallToolResult::success(vec![Content::text(format!("LoadTrack command sent to deck {deck} with filter '{filter}'"))]))
                }

                "set_crossfader" => {
                    let value = args.get("value")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| McpError::invalid_params("missing value", None))?;
                    let value = value.min(255) as u8;
                    self.bridge.lock().map_err(|_| McpError::internal_error("bridge lock poisoned", None))?
                        .commands.push_back(super::SimCmd::SetCrossfader(value));
                    Ok(CallToolResult::success(vec![Content::text(format!("Crossfader set to {value}"))]))
                }

                "list_tracks" => {
                    let bridge = self.bridge.lock().map_err(|_| {
                        McpError::internal_error("bridge lock poisoned", None)
                    })?;
                    let tracks: Vec<_> = bridge.available_tracks.iter().collect();
                    let result = json!({ "tracks": tracks });
                    Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
                }

                _ => Err(McpError::method_not_found::<rmcp::model::CallToolRequestMethod>()),
            }
        }
    }
}
