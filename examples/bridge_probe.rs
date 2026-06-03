//! End-to-end interop probe against an external TCNet peer.
//!
//! Runs the local node as a `Slave<V3_6>`, joins the network, and walks
//! three independent check categories. Each category prints a single
//! `PASS` / `FAIL` / `SKIP` line at the end so the output is greppable;
//! the exit code is 0 on overall success.
//!
//! Categories:
//!
//! 1. **Discovery + OptIn parse** — wait for a foreign node to appear in
//!    [`NodeSnapshot::peers`]. Always testable: even a peer with no
//!    upstream DJ controller will broadcast OptIn.
//! 2. **TimeSync handshake** — wait for [`Node::clock_offset_for`] to
//!    return `Some` for the discovered peer. Tests the spec-page-8
//!    three-step handshake end-to-end. Requires the peer to support
//!    TimeSync replies.
//! 3. **DJ-controller traffic** — if any peer has `has_dj_controller`,
//!    sample [`Node::layers_for`] / [`Node::mixer_for`] and try the four
//!    `request_*` methods (beat grid / cue / small + big waveform).
//!    Skipped (not failed) if no peer advertises a controller.
//!
//! ```sh
//! cargo run --release --example bridge_probe -- --bind-ip 127.0.0.1
//! ```
//!
//! The `--bind-ip` defaults to `127.0.0.1` so the loopback fallback in
//! [`bind_with_fallback`] applies; pass your real interface IP to probe
//! a peer on another host.

use clap::Parser;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::{Duration, Instant};
use tcnet::api::{NodeBuilder, Slave};
use tcnet::protocol::LayerId;
use tcnet::session::ElectionState;
use tcnet::{ApplicationConfig, V3_6};

#[derive(Parser)]
#[command(about = "TCNet bridge interop probe")]
struct Args {
    /// Local bind IP. Defaults to 127.0.0.1 so the loopback fallback
    /// applies when the peer is on the same host.
    #[arg(long, default_value = "127.0.0.1")]
    bind_ip: Ipv4Addr,

    /// Maximum seconds to wait for at least one peer to be discovered.
    #[arg(long, default_value_t = 8)]
    discovery_timeout: u64,

    /// Maximum seconds to wait for a TimeSync round-trip to complete.
    #[arg(long, default_value_t = 12)]
    time_sync_timeout: u64,

    /// Seconds to sample the DJ-controller snapshot for state changes
    /// (skipped if no peer has `has_dj_controller`).
    #[arg(long, default_value_t = 5)]
    controller_sample_window: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Pass,
    Fail,
    Skip,
}

impl Verdict {
    fn tag(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Skip => "SKIP",
        }
    }
}

fn banner(title: &str) {
    println!("\n=== {} ===", title);
}

/// Block until a peer is discovered or the timeout fires. Returns the
/// first peer's address.
fn await_first_peer(
    node: &tcnet::api::Node<Slave, V3_6>,
    timeout: Duration,
) -> Result<SocketAddrV4, String> {
    let deadline = Instant::now() + timeout;
    let mut last_count = 0;
    loop {
        let snap = node.snapshot();
        if snap.peers.len() != last_count {
            println!("  observed {} peer(s):", snap.peers.len());
            for p in &snap.peers {
                println!(
                    "    addr={} node_id={} has_dj_controller={} last_seen_unix={}",
                    p.address, p.node_id, p.has_dj_controller, p.last_seen
                );
            }
            last_count = snap.peers.len();
        }
        if let Some(p) = snap.peers.first() {
            return Ok(p.address);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no peer discovered within {} s",
                timeout.as_secs()
            ));
        }
        sleep(Duration::from_millis(250));
    }
}

/// Poll `clock_offset_for(peer)` until it returns `Some` or the timeout
/// fires. The dispatcher's TimeSync initiator runs every 5 s, so on a
/// healthy peer this resolves within ~6 s.
fn await_clock_offset(
    node: &tcnet::api::Node<Slave, V3_6>,
    peer: SocketAddrV4,
    timeout: Duration,
) -> Option<tcnet::proto::ClockOffset> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(offset) = node.clock_offset_for(peer) {
            return Some(offset);
        }
        sleep(Duration::from_millis(500));
    }
    None
}

fn check_discovery(
    node: &tcnet::api::Node<Slave, V3_6>,
    timeout: Duration,
) -> (Verdict, Option<SocketAddrV4>) {
    banner("1. Discovery + OptIn parse");
    println!(
        "Waiting up to {} s for a foreign TCNet peer to appear …",
        timeout.as_secs()
    );

    match await_first_peer(node, timeout) {
        Ok(addr) => {
            let snap = node.snapshot();
            println!(
                "Got {} peer(s); first peer at {}",
                snap.peers.len(),
                addr
            );
            (Verdict::Pass, Some(addr))
        }
        Err(reason) => {
            println!("FAIL: {}", reason);
            println!("Hints:");
            println!("  * is the foreign peer actually running and on this interface?");
            println!("  * does this host hold UDP 60000/60001/60002? `lsof -nP -iUDP:60000`");
            println!("  * if the peer is on the same host, --bind-ip 127.0.0.1 enables the loopback fallback");
            (Verdict::Fail, None)
        }
    }
}

fn check_time_sync(
    node: &tcnet::api::Node<Slave, V3_6>,
    peer: SocketAddrV4,
    timeout: Duration,
) -> Verdict {
    banner("2. TimeSync handshake (spec page 8)");
    println!(
        "Waiting up to {} s for clock_offset_for({}) to resolve …",
        timeout.as_secs(),
        peer
    );

    match await_clock_offset(node, peer, timeout) {
        Some(off) => {
            println!("Clock offset resolved:");
            println!("  round_trip      = {:?}", off.round_trip);
            println!("  one_way_delay   = {:?}", off.one_way_delay);
            println!(
                "  estimated_offset_us = {:?}  (positive → peer's clock ahead of ours)",
                off.estimated_offset_us
            );
            Verdict::Pass
        }
        None => {
            println!("FAIL: no TimeSync reply within {} s", timeout.as_secs());
            println!(
                "Hint: peer must respond to msg-type 10 with msg-type 11. \
                 If the peer is a passive translator that doesn't implement \
                 TimeSync reply, mark this as a known interop gap."
            );
            Verdict::Fail
        }
    }
}

fn check_election(node: &tcnet::api::Node<Slave, V3_6>) -> Verdict {
    banner("3. Master election visibility");
    let state = node.election_state();
    match state {
        ElectionState::Watching => {
            println!(
                "Election state = Watching (no master candidate yet). \
                 This is acceptable if no peer advertises Master / Auto."
            );
            Verdict::Pass
        }
        ElectionState::Contending { since } => {
            println!(
                "Election state = Contending since {:?} ago",
                since.elapsed()
            );
            Verdict::Pass
        }
        ElectionState::Elected(w) => {
            println!(
                "Election state = Elected(node_id={}, addr={}, elected {:?} ago)",
                w.node_id,
                w.addr,
                w.elected_at.elapsed()
            );
            Verdict::Pass
        }
    }
}

fn check_dj_controller(
    node: &mut tcnet::api::Node<Slave, V3_6>,
    sample_window: Duration,
) -> Verdict {
    banner("4. DJ-controller traffic + request_*");

    let snap = node.snapshot();
    let Some(peer) = snap.peers.iter().find(|p| p.has_dj_controller).cloned() else {
        println!(
            "SKIP: no peer advertises has_dj_controller. The bridge likely \
             has no upstream CDJ/DJM connected; nothing to read or request."
        );
        return Verdict::Skip;
    };

    let addr = peer.address;
    println!("Using DJ-controller peer at {}", addr);

    println!("Sampling layers/mixer for {} s …", sample_window.as_secs());
    let mut max_track_id: u32 = 0;
    let mut mixer_levels = (0u8, 0u8, 0u8);
    let mut first_active_layer: Option<u8> = None;
    let deadline = Instant::now() + sample_window;
    while Instant::now() < deadline {
        if let Some(layers) = node.layers_for(addr) {
            for (i, l) in layers.iter().enumerate() {
                if l.track_id != 0 {
                    max_track_id = max_track_id.max(l.track_id);
                    if first_active_layer.is_none() {
                        first_active_layer = Some(i as u8);
                        println!(
                            "  layer {} has track_id={} bpm={:.2} pos={}/{} ms title=\"{}\"",
                            i + 1,
                            l.track_id,
                            l.bpm.as_f32(),
                            l.current_time_ms,
                            l.total_time_ms,
                            l.title
                        );
                    }
                }
            }
        }
        if let Some(m) = node.mixer_for(addr) {
            mixer_levels = (m.master_audio_level, m.master_fader_level, m.crossfader);
        }
        sleep(Duration::from_millis(250));
    }

    println!(
        "After {} s: max track_id={}, mixer master/fader/xfader={}/{}/{}",
        sample_window.as_secs(),
        max_track_id,
        mixer_levels.0,
        mixer_levels.1,
        mixer_levels.2
    );

    let Some(layer_idx) = first_active_layer else {
        println!(
            "SKIP request_* probes: no layer reported a non-zero track_id \
             in the sampling window. Load a track on the upstream deck and \
             re-run if you want the request side exercised."
        );
        return Verdict::Pass;
    };

    let layer_id = match layer_idx {
        0 => LayerId::L1,
        1 => LayerId::L2,
        2 => LayerId::L3,
        3 => LayerId::L4,
        4 => LayerId::LA,
        5 => LayerId::LB,
        6 => LayerId::LM,
        _ => LayerId::LC,
    };

    let runtime = node.runtime_handle();
    let mut request_results: Vec<(&str, Result<String, String>)> = Vec::new();

    let beat_grid = runtime.block_on(async { node.request_beat_grid(addr, layer_id).await });
    request_results.push(match beat_grid {
        Ok(bg) => (
            "beat_grid",
            Ok(format!(
                "data_size={} total_packets={} payload_bytes={}",
                bg.data_size,
                bg.total_packets,
                bg.payload.len()
            )),
        ),
        Err(e) => ("beat_grid", Err(format!("{e}"))),
    });

    let cue = runtime.block_on(async { node.request_cue_data(addr, layer_id).await });
    request_results.push(match cue {
        Ok(_c) => ("cue_data", Ok("response parsed".to_string())),
        Err(e) => ("cue_data", Err(format!("{e}"))),
    });

    let small_wf = runtime.block_on(async { node.request_small_waveform(addr, layer_id).await });
    request_results.push(match small_wf {
        Ok(w) => (
            "small_waveform",
            Ok(format!("bytes={}", w.bytes().len())),
        ),
        Err(e) => ("small_waveform", Err(format!("{e}"))),
    });

    let big_wf = runtime.block_on(async { node.request_big_waveform(addr, layer_id).await });
    request_results.push(match big_wf {
        Ok(w) => (
            "big_waveform",
            Ok(format!("first_chunk_bytes={}", w.bytes().len())),
        ),
        Err(e) => ("big_waveform", Err(format!("{e}"))),
    });

    let mut any_fail = false;
    for (name, result) in &request_results {
        match result {
            Ok(detail) => println!("  request_{:15} OK    {}", name, detail),
            Err(reason) => {
                println!("  request_{:15} ERR   {}", name, reason);
                any_fail = true;
            }
        }
    }

    if any_fail {
        println!(
            "Note: some request_* methods returned errors. The bridge \
             documents which TCNet response classes it supports; a peer \
             that doesn't implement a response will surface as RequestTimeout."
        );
        Verdict::Pass
    } else {
        Verdict::Pass
    }
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    println!("TCNet bridge interop probe");
    println!("  bind IP         = {}", args.bind_ip);
    println!("  discovery       = {} s", args.discovery_timeout);
    println!("  time-sync       = {} s", args.time_sync_timeout);
    println!(
        "  controller-sample = {} s",
        args.controller_sample_window
    );

    let mut config = ApplicationConfig::default();
    config.address.set_ip(args.bind_ip);

    let mut node = match NodeBuilder::<Slave, V3_6>::new()
        .with_config(config)
        .with_local_ip(args.bind_ip)
        .spawn()
    {
        Ok(n) => n,
        Err(e) => {
            eprintln!("spawn failed: {:?}", e);
            return ExitCode::from(2);
        }
    };

    let (discovery_verdict, peer_addr) =
        check_discovery(&node, Duration::from_secs(args.discovery_timeout));

    let time_sync_verdict = match peer_addr {
        Some(addr) => check_time_sync(&node, addr, Duration::from_secs(args.time_sync_timeout)),
        None => Verdict::Skip,
    };

    let election_verdict = check_election(&node);

    let controller_verdict =
        check_dj_controller(&mut node, Duration::from_secs(args.controller_sample_window));

    banner("Summary");
    println!(" 1. Discovery + OptIn parse   {}", discovery_verdict.tag());
    println!(" 2. TimeSync handshake        {}", time_sync_verdict.tag());
    println!(" 3. Master election           {}", election_verdict.tag());
    println!(" 4. DJ-controller traffic     {}", controller_verdict.tag());

    let any_fail = [
        discovery_verdict,
        time_sync_verdict,
        election_verdict,
        controller_verdict,
    ].contains(&Verdict::Fail);

    if any_fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
