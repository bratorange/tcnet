use clap::Parser;
use std::net::Ipv4Addr;
use std::thread::sleep;
use std::time::Duration;
use tcnet::{ApplicationConfig, DjControllerView, TCNetClient};

#[derive(Parser)]
struct Args {
    binding_ip: Ipv4Addr,
}

fn print_state(view: &mut DjControllerView) {
    // get_layers and get_mixer each reborrow view mutably, so we use separate blocks.
    {
        let layers = view.get_layers();
        let active: Vec<_> = layers.iter().enumerate()
            .filter(|(_, l)| l.track_id != 0)
            .collect();

        if active.is_empty() {
            println!("[layers] no tracks loaded");
        } else {
            for (i, layer) in active {
                println!(
                    "[L{}] {:?} | {:>6.1} bpm | {:>5.1}% | pos {:>8}ms / {:>8}ms | \"{}\" — \"{}\"",
                    i + 1,
                    layer.state,
                    layer.bpm.as_f32(),
                    layer.speed.as_percent(),
                    layer.current_time_ms,
                    layer.total_time_ms,
                    layer.artist,
                    layer.title,
                );
            }
        }
    }

    {
        let m = view.get_mixer();
        if m.mixer_id != 0 {
            println!(
                "[mixer] master={:3} fader={:3} xfader={:3}",
                m.master_audio_level, m.master_fader_level, m.crossfader
            );
        }
    }

    println!();
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut client = TCNetClient::new(args.binding_ip, ApplicationConfig::default());

    // Wait until a foreign node advertising a DJ controller is discovered.
    let mut view = loop {
        sleep(Duration::from_secs(1));

        let nodes = client.active_nodes().to_vec();
        println!("Discovered {} node(s)…", nodes.len());
        for n in &nodes {
            println!("  {} | has_dj_controller={}", n.address, n.has_dj_controller);
        }

        if let Some(node) = nodes.iter().find(|n| n.has_dj_controller) {
            if let Some(view) = client.get_controller_view(node.address) {
                println!("Got DjControllerView for {}\n", node.address);
                break view;
            }
        }
    };

    loop {
        sleep(Duration::from_millis(500));
        print_state(&mut view);
    }
}
