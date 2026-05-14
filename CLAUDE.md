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

# Build + launch simulator via the .app wrapper (required for computer-use)
make run-simulator            # builds, copies binary, relaunches /Applications/DJSimulator.app
make stop-simulator           # kill the running instance
```

## GUI Verification Workflow (computer-use)

The simulator runs as a proper macOS `.app` bundle so that Claude Code's `computer-use` MCP can grant it screen-capture access via ScreenCaptureKit.

**One-time setup** (already done — do not repeat):
- `/Applications/DJSimulator.app` exists with bundle ID `com.tcnet.djsimulator`
- It was registered with LaunchServices and ad-hoc signed

**Each dev iteration** — use `make run-simulator` instead of `cargo run`:
```bash
make run-simulator            # builds, copies binary into .app, relaunches
# Override defaults:
make run-simulator BIND_IP=192.168.1.100 USB_DIR=~/Music
```

**Each Claude Code session** — at the start of any session involving the simulator, call:
```
request_access(apps=["DJSimulator"])   # grants com.tcnet.djsimulator
```
No Claude restart is needed after this — the bundle ID is stable and permanently installed.

**Implementation files:**
- `src/bin/simulator.rs` — creates `McpClient`, spawns `IpcServer` thread, enables AccessKit via `cc.egui_ctx.enable_accesskit()`
- `src/simulator/app.rs` — egui `update` loop, UI layout

**Note:** egui-mcp (`mcp__egui-mcp__*` tools) does not work on macOS (requires Linux AT-SPI). Use computer-use instead.

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