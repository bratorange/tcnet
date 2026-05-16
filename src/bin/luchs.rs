use clap::Parser;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use egui_mcp_client::{IpcServer, McpClient};
use tcnet::luchs::app::LuchsApp;
use tcnet::{ApplicationConfig, NodeType, TCNetClient};

#[derive(Parser)]
#[command(name = "luchs", about = "LUCHS — VJ phrase-annotator over TCNet")]
struct Args {
    /// IP address to bind the TCNet node to
    #[arg(long, default_value = "0.0.0.0")]
    bind_ip: Ipv4Addr,

    /// Folder containing audio files mirrored from the mixer USB
    #[arg(long, default_value = ".")]
    media_dir: PathBuf,
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut node_config = ApplicationConfig::default();
    node_config.node_type = NodeType::Slave;
    node_config.address.set_ip(args.bind_ip);
    let client = TCNetClient::new(node_config);

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

    let bind_ip_label = args.bind_ip.to_string();
    // Locate scripts/ relative to the crate (when run from cargo it's the
    // repo root; when run from the binary it's two directories up).
    let script_dir = locate_scripts_dir();

    let app = LuchsApp::new(client, bind_ip_label, args.media_dir, script_dir, mcp_client, rt);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LUCHS — annotator")
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([1080.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "LUCHS",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.enable_accesskit();
            Ok(Box::new(app))
        }),
    )
    .expect("Failed to start eframe");
}

/// Locate the in-repo `scripts/` directory by walking up from the binary
/// location. Falls back to `./scripts`.
fn locate_scripts_dir() -> PathBuf {
    let mut dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    for _ in 0..6 {
        let candidate = dir.join("scripts").join("luchs_allin1.py");
        if candidate.exists() {
            return dir.join("scripts");
        }
        if !dir.pop() {
            break;
        }
    }
    PathBuf::from("scripts")
}
