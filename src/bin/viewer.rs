use clap::Parser;
use std::net::Ipv4Addr;
use tcnet::viewer::app::ViewerApp;
use tcnet::{ApplicationConfig, NodeType, TCNetClient};

#[derive(Parser)]
#[command(name = "viewer", about = "TCNet DJ controller passive viewer")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    bind_ip: Ipv4Addr,
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut node_config = ApplicationConfig::default();
    node_config.node_type = NodeType::Slave;
    node_config.address.set_ip(args.bind_ip);

    let client = TCNetClient::new(node_config);
    let app = ViewerApp::new(client);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TCNet Viewer")
            .with_inner_size([1400.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "TCNet Viewer",
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )
    .expect("Failed to start eframe");
}
