//! Example binary that passively listens for a foreign TCNet DJ controller
//! and prints its layer + mixer state every 500 ms.
//!
//! Run with the bind IP of the local network interface:
//!
//! ```sh
//! cargo run --example observer -- 192.168.1.42
//! ```
//!
//! Useful as a quick connectivity / discovery test and as a minimum-viable
//! reference for building a passive observer with this crate.

use clap::Parser;
use log::trace;
use std::net::Ipv4Addr;
use std::thread::sleep;
use std::time::Duration;
use tcnet::api::{NodeBuilder, Slave};
use tcnet::{ApplicationConfig, LayerSnapshot, MixerSnapshot, V3_6};

#[derive(Parser)]
struct Args {
    binding_ip: Ipv4Addr,
}

fn print_state(layers: &[LayerSnapshot], mixer: &MixerSnapshot) {
    let active: Vec<_> = layers
        .iter()
        .enumerate()
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

    if mixer.mixer_id != 0 {
        println!(
            "[mixer] master={:3} fader={:3} xfader={:3}",
            mixer.master_audio_level, mixer.master_fader_level, mixer.crossfader
        );
    }

    println!();
}

fn main() {
    env_logger::init();
    let args = Args::parse();
    let bind_address = args.binding_ip;

    let mut config = ApplicationConfig::default();
    config.address.set_ip(bind_address);

    let mut node = NodeBuilder::<Slave, V3_6>::new()
        .with_config(config)
        .with_local_ip(bind_address)
        .spawn()
        .expect("node spawn");

    // Wait until a foreign node advertising a DJ controller is discovered.
    let peer_addr = loop {
        sleep(Duration::from_secs(1));

        let snap = node.snapshot();
        println!("Discovered {} node(s)…", snap.peers.len());
        for p in &snap.peers {
            println!(
                "  {} | has_dj_controller={}",
                p.address, p.has_dj_controller
            );
        }

        if let Some(p) = snap.peers.iter().find(|n| n.has_dj_controller) {
            println!("Found DJ controller at {}\n", p.address);
            break p.address;
        }
    };

    loop {
        let snap = node.snapshot();
        println!("Discovered {} node(s)…", snap.peers.len());
        sleep(Duration::from_millis(500));
        let layers = node.layers_for(peer_addr).unwrap_or_default();
        let mixer = node.mixer_for(peer_addr).unwrap_or_default();
        print_state(&layers, &mixer);
        trace!("{:?}", mixer);
    }
}
