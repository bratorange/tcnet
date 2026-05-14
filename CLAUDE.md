# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                   # debug build
cargo build --release         # release build
cargo run -- <bind_ip>        # run (bind_ip is a required CLI arg, e.g. 192.168.1.100)
cargo test                    # run tests
cargo clippy                  # lint
cargo fmt                     # format

# Build + run simulator (plain binary, for egui-mcp automation — primary workflow)
make run-simulator-mcp        # builds, kills old instance, starts ./target/debug/simulator in bg
make stop-simulator           # kill the running instance

# Build + launch simulator via the .app wrapper (required for computer-use / ScreenCaptureKit)
make run-simulator            # builds, copies binary, relaunches /Applications/DJSimulator.app
```

## egui-mcp on macOS

egui-mcp gives Claude semantic UI access (find/click/screenshot widgets by label, role, or ID) without AT-SPI (Linux-only) or ScreenCaptureKit. It works via a Unix socket at `/tmp/egui-mcp.sock`.

### How it works

Each frame, the eframe app captures the egui AccessKit tree via an egui `Plugin::output_hook` (which fires after `end_pass()` sets `platform_output.accesskit_update`), converts it to our `UiTree` type, and stores it in the `McpClient`. The `IpcServer` (running on a background thread) handles `GetUiTree` requests from the `egui-mcp-server` process. The server process is started by Claude Code via `.mcp.json`.

**Key insight:** Reading `ctx.output().accesskit_update` inside `App::update()` always returns `None` — the tree is only built by egui's internal `end_pass()`, which runs *after* `update()` returns. Use `Plugin::output_hook` instead.

### Wiring egui-mcp into an eframe app

1. **Cargo deps** — add to the app's feature in `Cargo.toml`:
   ```toml
   "dep:egui-mcp-client", "dep:egui-mcp-protocol", "egui/accesskit", "dep:accesskit"
   ```
   The `[patch.crates-io]` block already redirects these to `local_crates/`.

2. **Binary entry point** — in `src/bin/<app>.rs`:
   ```rust
   use egui_mcp_client::{IpcServer, McpClient};

   let mcp_client = McpClient::new();
   let mcp_client_for_ipc = mcp_client.clone();
   let ipc_rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
   ipc_rt.spawn(async move {
       if let Err(e) = IpcServer::run(mcp_client_for_ipc).await {
           eprintln!("egui-mcp IPC error: {e}");
       }
   });
   // Pass mcp_client + ipc_rt to the App struct
   ```

3. **App struct** — add these fields:
   ```rust
   mcp_client: McpClient,
   rt: tokio::runtime::Runtime,          // tokio rt for block_on calls
   pending_tree: Arc<Mutex<Option<UiTree>>>,
   plugin_registered: bool,
   ```

4. **Plugin** — define once (e.g. at top of `app.rs`):
   ```rust
   struct AccessKitCapturePlugin { pending: Arc<Mutex<Option<UiTree>>> }

   impl egui::Plugin for AccessKitCapturePlugin {
       fn debug_name(&self) -> &'static str { "AccessKitCapture" }
       fn output_hook(&mut self, output: &mut egui::FullOutput) {
           if let Some(tree) = &output.platform_output.accesskit_update {
               if let Ok(mut g) = self.pending.lock() {
                   *g = Some(crate::<module>::accesskit_tree::convert(tree));
               }
           }
       }
   }
   ```
   See `src/simulator/accesskit_tree.rs` for the `convert()` implementation.

5. **`App::update()` — top of the function:**
   ```rust
   if !self.plugin_registered {
       ctx.add_plugin(AccessKitCapturePlugin { pending: self.pending_tree.clone() });
       self.plugin_registered = true;
   }
   if let Ok(mut g) = self.pending_tree.lock() {
       if let Some(tree) = g.take() {
           let _ = self.rt.block_on(self.mcp_client.set_ui_tree(tree));
       }
   }
   ```

6. **Widget accessibility** — `find_by_label` only works if widgets register info. Standard egui widgets (`ui.button(...)`, `ui.add(Slider...)`, etc.) do this automatically. For custom `allocate_rect`-based widgets, add:
   ```rust
   let resp = ui.allocate_rect(rect, Sense::click());
   resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "MY LABEL"));
   // or for sliders:
   resp.widget_info(|| egui::WidgetInfo::slider(true, value as f64, "MY SLIDER"));
   ```

### Running and verifying

Start the app as a plain binary (no `.app` bundle needed):
```bash
RUST_LOG=warn ./target/debug/<binary> --bind-ip 127.0.0.1 &
```

Then verify:
```
mcp__egui-mcp__ping                              → pong
mcp__egui-mcp__check_connection                  → connected
mcp__egui-mcp__get_ui_tree                       → full widget tree JSON
mcp__egui-mcp__find_by_label {"pattern": "X"}   → elements with matching label
mcp__egui-mcp__click_element {"id": "<id>"}      → click at element center
mcp__egui-mcp__take_screenshot                   → PNG of app window
```

The `.mcp.json` is already configured to use `local_crates/egui-mcp-server/target/debug/egui-mcp-server`.

### Simulator-specific commands

```bash
make run-simulator-mcp   # build + kill old + start ./target/debug/simulator in bg
make stop-simulator      # kill running instance
make run-simulator       # .app bundle variant (for computer-use / ScreenCaptureKit)

**Each dev iteration:**
```bash
make run-simulator   # builds, copies binary into .app, relaunches
```

## Architecture

`tcnet-rs` is a Rust implementation of the TCNet protocol (v3.6) for networked DJ equipment. It can both observe TCNet traffic and emulate an active DJ controller (CDJ).

### Entry Points

- **Binary** (`src/main.rs`): parses a bind IP via clap, creates a `TCNetClient`, runs forever.
- **Library** (`src/lib.rs`): exposes `TCNetClient`, which manages a tokio runtime and spawns a `Dispatcher`.

### Module Layout

```
src/
  lib.rs                        public API: TCNetClient
  main.rs                       CLI entry point
  node/
    mod.rs                      ForeignNode, ApplicationConfig, DynamicNodeState
    dispatcher.rs               core: socket management, node discovery, routing
    tcnet_packet.rs             Data enum (19 variants), deserialization dispatch
    tcnet_packet_serde.rs       all binary packet structs (deku-based, ~727 lines)
  application/
    mod.rs                      ApplicationNode trait, ApplicationMessage struct
    dj_controller_view.rs       passive observer: LayerSnapshot × 8, MixerSnapshot
    active_dj_controller.rs     active CDJ emulator: play/pause/stop, broadcasts
```

### Data Flow

**Discovery:** Foreign nodes send OptIn packets → `Dispatcher` receives on port 60000 → updates `DynamicNodeState.discovered_nodes`.

**Passive (receive):** Packets arrive on ports 60000/60001/60002/65023 → `Dispatcher.listen()` → fan-out to all `ApplicationNode`s via kanal channels → `DjControllerView.apply()` updates layer/mixer snapshots.

**Active (send):** `ApplicationMessage` enqueued on kanal sender → `Dispatcher.send()` task → `UdpSocket.send_to()` each discovered node.

### Key Design Patterns

- **Packet dispatch** (`tcnet_packet.rs`): `Data` enum dispatches on `message_type_id` (and sub-type for type 200). Manual `DekuWrite`/`DekuReader` implementations exist for `NodeOptions`.
- **State** (`DynamicNodeState`, `LayerControl`): `Arc<RwLock<HashMap>>` shared across async tasks.
- **Channels**: kanal bounded channels (capacity 100); `drain_into()` for non-blocking polling.
- **Binary layout** (`tcnet_packet_serde.rs`): `deku` attributes on structs; `AsciiString<N>`, `ReservedData<N>` generics; `into_ascii!()` macro for fixed-size string constants.

### Network Ports (TCNet Spec)

| Port  | Purpose                     |
|-------|-----------------------------|
| 60000 | Broadcast status/control    |
| 60001 | Broadcast time packets      |
| 60002 | Reserved broadcast          |
| 65023 | Unicast (configurable)      |

### Timing

- OptIn broadcast: every 1 s (required by spec)
- Time packets: every 20 ms
- Status packets: every 1 s
- Metrics polling: every 50 ms
- Foreign node timeout: 10 s of silence