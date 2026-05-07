use clap::Parser;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use tcnet::simulator::app::SimulatorApp;
use tcnet::simulator::audio::AudioEngine;
use tcnet::simulator::virtual_usb::VirtualUsb;
use tcnet::{ApplicationConfig, NodeType, TCNetClient};

#[derive(Parser)]
#[command(name = "simulator", about = "DJ Deck simulator")]
struct Args {
    /// IP address to bind the TCNet node to
    #[arg(long, default_value = "0.0.0.0")]
    bind_ip: Ipv4Addr,

    /// Folder to use as the virtual USB stick (scanned for audio files)
    #[arg(long, default_value = ".")]
    usb_dir: PathBuf,
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
    let app = SimulatorApp::new(active_node, usb, audio);

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
        Box::new(move |_cc| Ok(Box::new(app))),
    ).expect("Failed to start eframe");

    // Keep TCNetClient alive for the duration of the app
    drop(client);
}
