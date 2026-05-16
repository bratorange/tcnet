use clap::Parser;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tcnet::simulator::app::SimulatorApp;
use tcnet::simulator::audio::AudioEngine;
use tcnet::simulator::mcp::SimBridge;
use tcnet::simulator::virtual_usb::VirtualUsb;
use tcnet::{ApplicationConfig, NodeType, TCNetClient};
use egui_mcp_client::{IpcServer, McpClient};

#[derive(Parser)]
#[command(name = "simulator", about = "DJ Deck simulator")]
struct Args {
    /// IP address to bind the TCNet node to
    #[arg(long, default_value = "0.0.0.0")]
    bind_ip: Ipv4Addr,

    /// Folder to use as the virtual USB stick (scanned for audio files)
    #[arg(long, default_value = ".")]
    usb_dir: PathBuf,

    /// Enable MCP stdio server for remote control
    #[arg(long)]
    mcp: bool,
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut node_config = ApplicationConfig::default();
    node_config.node_type = NodeType::Master;
    node_config.address.set_ip(args.bind_ip);
    let client = TCNetClient::new(node_config);
    let active_node = client.create_active_node();

    let usb = VirtualUsb::from_dir(args.usb_dir);
    let audio = AudioEngine::new();

    let bridge: Option<Arc<Mutex<SimBridge>>> = if args.mcp {
        let bridge = Arc::new(Mutex::new(SimBridge::default()));

        #[cfg(feature = "mcp")]
        {
            let bridge_clone = Arc::clone(&bridge);
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(async move {
                    use rmcp::ServiceExt;
                    use tcnet::simulator::mcp::server::SimMcpServer;
                    let server = SimMcpServer::new(bridge_clone);
                    let transport = rmcp::transport::io::stdio();
                    match server.serve(transport).await {
                        Ok(running) => {
                            let _ = running.waiting().await;
                        }
                        Err(e) => {
                            eprintln!("MCP server init error: {e}");
                        }
                    }
                });
            });
        }
        #[cfg(not(feature = "mcp"))]
        {
            eprintln!("--mcp flag requires the 'mcp' feature to be enabled at compile time");
        }

        Some(bridge)
    } else {
        None
    };

    // Set up egui-mcp-client: IPC server for screenshot + input injection
    let mcp_client = McpClient::new();
    let mcp_client_for_ipc = mcp_client.clone();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("egui-mcp tokio runtime");
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("egui-mcp ipc tokio runtime");
        rt.block_on(async move {
            if let Err(e) = IpcServer::run(mcp_client_for_ipc).await {
                eprintln!("egui-mcp IPC server error: {e}");
            }
        });
    });

    let script_dir = locate_scripts_dir();
    let app = SimulatorApp::new(active_node, usb, audio, script_dir, bridge, mcp_client, rt);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DJ Deck Simulator")
            .with_inner_size([1130.0, 660.0])
            .with_min_inner_size([1100.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "CDJ Simulator",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.enable_accesskit();
            Ok(Box::new(app))
        }),
    ).expect("Failed to start eframe");

    drop(client);
}

/// Locate the in-repo `scripts/` directory by walking up from the binary
/// location. Falls back to `./scripts`.
fn locate_scripts_dir() -> PathBuf {
    let mut dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    for _ in 0..6 {
        let candidate = dir.join("scripts").join("sim_beatgrid.py");
        if candidate.exists() {
            return dir.join("scripts");
        }
        if !dir.pop() {
            break;
        }
    }
    PathBuf::from("scripts")
}
