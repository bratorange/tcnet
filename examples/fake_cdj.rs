//! Synthetic Pro DJ Link emitter — a fake CDJ-2000nexus on the
//! upstream side of the TCNet ↔ Pro DJ Link bridge.
//!
//! Sends the three packet types that a CDJ produces during playback,
//! all per the Deep Symmetry djl-analysis paper:
//!
//! - **Type `0x06` Device Keep-Alive** → broadcast every 1.5 s to port
//!   `50000`. 54 bytes. Tells the bridge a CDJ exists.
//! - **Type `0x0a` CDJ Status** → broadcast every 100 ms to port
//!   `50002`. 208 bytes (CDJ-2000nexus / subtype `0x03`). Carries
//!   rekordbox metadata: track ID, BPM, beat-in-bar, play state.
//! - **Type `0x28` Beat broadcast** → broadcast at the BPM cadence
//!   (`60000 / BPM` ms) to port `50001`. 96 bytes. Tells the bridge
//!   the beat is happening *now*.
//!
//! With all three feeding, the bridge should surface a stable,
//! track-bearing TCNet ghost peer on the downstream side, allowing
//! `request_*` paths to be exercised against the bridge.
//!
//! References (all from the same source page):
//! - Beat layout: <https://djl-analysis.deepsymmetry.org/djl-analysis/beats.html>
//! - Status layout: <https://djl-analysis.deepsymmetry.org/djl-analysis/vcdj.html>
//!
//! ```sh
//! cargo run --release --example fake_cdj -- --device-number 1 \
//!     --device-name CDJ-3000 --bpm 128 --track-id 42 \
//!     --status-target 127.0.0.1:50002 --beat-target 127.0.0.1:50001 \
//!     --keepalive-target 127.0.0.1:50000
//! ```

use clap::Parser;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const MAGIC: &[u8; 10] = b"Qspt1WmJOL";

#[derive(Parser, Clone)]
#[command(about = "Pro DJ Link fake CDJ — keep-alive + status + beat emitter")]
struct Args {
    /// Pro DJ Link device number, 1-4.
    #[arg(long, default_value_t = 1)]
    device_number: u8,
    /// Device name, padded / truncated to 20 bytes ASCII.
    #[arg(long, default_value = "CDJ-2000nexus")]
    device_name: String,
    /// Track BPM. Used to set the BPM field in status + beat packets,
    /// and the beat-broadcast cadence (60000 / BPM ms).
    #[arg(long, default_value_t = 128.0)]
    bpm: f32,
    /// Rekordbox track ID. Non-zero so the bridge can surface a real
    /// "track loaded" state to its downstream TCNet consumers.
    #[arg(long, default_value_t = 42)]
    track_id: u32,
    /// Where to send keep-alive (UDP 50000 by default).
    #[arg(long, default_value = "127.0.0.1:50000")]
    keepalive_target: SocketAddrV4,
    /// Where to send beat broadcasts (UDP 50001 by default).
    #[arg(long, default_value = "127.0.0.1:50001")]
    beat_target: SocketAddrV4,
    /// Where to send CDJ status packets (UDP 50002 by default).
    #[arg(long, default_value = "127.0.0.1:50002")]
    status_target: SocketAddrV4,
    /// Source IP advertised inside the packet body.
    #[arg(long, default_value = "127.0.0.1")]
    advertised_ip: Ipv4Addr,
    /// Synthetic MAC address advertised inside the packet, hex no separators.
    #[arg(long, default_value = "020203040506")]
    advertised_mac: String,
    /// Stop after N seconds. 0 = run forever.
    #[arg(long, default_value_t = 0)]
    duration_secs: u64,
    /// Whether to set the Master flag in the status packet flags byte.
    #[arg(long, default_value_t = true)]
    master_flag: bool,
    /// Whether to set the Sync flag in the status packet flags byte.
    #[arg(long, default_value_t = true)]
    sync_flag: bool,
}

/// Mutable shared state read by the status emitter, written by the
/// beat emitter.
struct CdjState {
    /// Current beat within bar, `1..=4`. Bumped by the beat thread.
    beat_in_bar: AtomicU8,
    /// Total beats elapsed since start. Bumped by the beat thread,
    /// emitted in status packets.
    beats_elapsed: AtomicU32,
    /// Packet counter for the status packet — incremented per emit.
    status_packet_counter: AtomicU32,
}

fn pad_name(buf: &mut [u8], name: &str) {
    let bytes = name.as_bytes();
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
}

fn parse_mac(hex: &str) -> [u8; 6] {
    let mut out = [0u8; 6];
    if hex.len() == 12 {
        for i in 0..6 {
            out[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0);
        }
    }
    out
}

/// Type `0x06` Device Keep-Alive, 54 bytes.
fn build_keepalive(args: &Args) -> Vec<u8> {
    let mut pkt = vec![0u8; 54];
    pkt[0..10].copy_from_slice(MAGIC);
    pkt[10] = 0x06;
    pkt[11] = 0x00; // subtype
    pad_name(&mut pkt[12..32], &args.device_name);
    pkt[32] = 0x01;
    pkt[33] = 0x02;
    pkt[34] = args.device_number;
    pkt[35] = 0x36; // total length 54
    pkt[36..42].copy_from_slice(&parse_mac(&args.advertised_mac));
    pkt[42..46].copy_from_slice(&args.advertised_ip.octets());
    pkt[46] = 0x02;
    pkt[50] = 0x01; // device type = CDJ
    pkt
}

/// Type `0x0a` CDJ Status packet, CDJ-2000nexus variant (subtype 0x03,
/// 208 bytes). Layout per
/// <https://djl-analysis.deepsymmetry.org/djl-analysis/vcdj.html>.
///
/// Fields populated (the rest are zeroed):
/// - 0x00..0x09 magic
/// - 0x0A type 0x0a, 0x0B subtype 0x03
/// - 0x0C..0x1F device name (20B NUL-padded)
/// - 0x20 const 0x01, 0x21 device number D
/// - 0x22..0x23 `lenr` (BE u16) = 0x00B0 (counts from 0x20 inclusive)
/// - 0x27 activity = 0x01 (playing)
/// - 0x28 Dr (track source CDJ), 0x29 Sr (slot=3=USB), 0x2A Tr (type=1=rekordbox)
/// - 0x2C..0x2F rekordbox track ID (BE u32)
/// - 0x89 status flags (bit 5 master, bit 4 sync, bit 6 play)
/// - 0x92..0x93 BPM × 100 (BE u16)
/// - 0xA0..0xA3 beats elapsed (BE u32)
/// - 0xA4..0xA5 beats to next cue (BE u16) = 0x01FF (none/far)
/// - 0xA6 beat in bar (1..=4)
/// - 0xC8..0xCB packet counter (BE u32)
fn build_status(args: &Args, state: &CdjState) -> Vec<u8> {
    let mut pkt = vec![0u8; 0xD0]; // 208 bytes
    pkt[0..10].copy_from_slice(MAGIC);
    pkt[0x0A] = 0x0A;
    pkt[0x0B] = 0x03; // subtype: CDJ
    pad_name(&mut pkt[0x0C..0x20], &args.device_name);
    pkt[0x20] = 0x01;
    pkt[0x21] = args.device_number;
    // lenr counts from 0x20 inclusive: 208 - 32 = 176 = 0xB0.
    let lenr: u16 = 0x00B0;
    pkt[0x22..0x24].copy_from_slice(&lenr.to_be_bytes());

    pkt[0x24] = args.device_number;
    pkt[0x27] = 0x01; // activity = playing
    pkt[0x28] = args.device_number; // track source CDJ
    pkt[0x29] = 0x03; // slot = USB
    pkt[0x2A] = 0x01; // type = rekordbox

    pkt[0x2C..0x30].copy_from_slice(&args.track_id.to_be_bytes());

    // Play Mode byte P1 at 0x4B: 0x03 = playing, 0x05 = paused.
    pkt[0x4B] = 0x03;

    // Status flags byte at 0x89.
    let mut flags: u8 = 0;
    flags |= 1 << 6; // playing
    if args.master_flag {
        flags |= 1 << 5;
    }
    if args.sync_flag {
        flags |= 1 << 4;
    }
    pkt[0x89] = flags;

    // BPM × 100 as BE u16 at 0x92..0x93. Clamp to u16 range.
    let bpm_x100: u16 = ((args.bpm * 100.0).clamp(0.0, u16::MAX as f32)) as u16;
    pkt[0x92..0x94].copy_from_slice(&bpm_x100.to_be_bytes());

    let beats = state.beats_elapsed.load(Ordering::Relaxed);
    pkt[0xA0..0xA4].copy_from_slice(&beats.to_be_bytes());
    pkt[0xA4..0xA6].copy_from_slice(&0x01FFu16.to_be_bytes()); // beats-to-cue: none/far
    pkt[0xA6] = state.beat_in_bar.load(Ordering::Relaxed);

    let pc = state.status_packet_counter.fetch_add(1, Ordering::Relaxed) + 1;
    pkt[0xC8..0xCC].copy_from_slice(&pc.to_be_bytes());

    pkt
}

/// Type `0x28` beat broadcast, 96 bytes (0x60). Layout per
/// <https://djl-analysis.deepsymmetry.org/djl-analysis/beats.html>.
///
/// Note device name spans `0x0B..0x1F` (21 bytes, NUL-padded) per the
/// doc — unlike keep-alive/status which use `0x0C..0x1F`. Subtype sits
/// at `0x20`, *not* at `0x0B`.
fn build_beat(args: &Args, state: &CdjState) -> Vec<u8> {
    let mut pkt = vec![0u8; 0x60]; // 96 bytes
    pkt[0..10].copy_from_slice(MAGIC);
    pkt[0x0A] = 0x28;
    pad_name(&mut pkt[0x0B..0x20], &args.device_name);
    pkt[0x20] = 0x00; // subtype
    pkt[0x21] = args.device_number;
    let lenr: u16 = 0x003C; // 60 = bytes after lenr field
    pkt[0x22..0x24].copy_from_slice(&lenr.to_be_bytes());

    // Beat-cadence look-ahead: ms until next beat / 2nd beat / next bar / etc.
    let ms_per_beat = if args.bpm > 0.0 {
        (60_000.0 / args.bpm) as u32
    } else {
        500
    };
    let ms_per_bar = ms_per_beat * 4;
    let next_beat = ms_per_beat;
    let second_beat = ms_per_beat * 2;
    let next_bar = ms_per_bar;
    let fourth_beat = ms_per_beat * 4;
    let second_bar = ms_per_bar * 2;
    let eighth_beat = ms_per_beat * 8;
    pkt[0x24..0x28].copy_from_slice(&next_beat.to_be_bytes());
    pkt[0x28..0x2C].copy_from_slice(&second_beat.to_be_bytes());
    pkt[0x2C..0x30].copy_from_slice(&next_bar.to_be_bytes());
    pkt[0x30..0x34].copy_from_slice(&fourth_beat.to_be_bytes());
    pkt[0x34..0x38].copy_from_slice(&second_bar.to_be_bytes());
    pkt[0x38..0x3C].copy_from_slice(&eighth_beat.to_be_bytes());

    // Padding 0xFF at 0x3C..0x53 per the spec.
    for b in &mut pkt[0x3C..0x54] {
        *b = 0xFF;
    }

    // Pitch (no adjustment) = 0x00100000 BE u32 at 0x54..0x57.
    pkt[0x54..0x58].copy_from_slice(&0x0010_0000u32.to_be_bytes());

    // BPM × 100 at 0x5A..0x5B (BE u16).
    let bpm_x100: u16 = ((args.bpm * 100.0).clamp(0.0, u16::MAX as f32)) as u16;
    pkt[0x5A..0x5C].copy_from_slice(&bpm_x100.to_be_bytes());

    // Beat in bar at 0x5C.
    pkt[0x5C] = state.beat_in_bar.load(Ordering::Relaxed);

    // Device number copy at 0x5F.
    pkt[0x5F] = args.device_number;

    pkt
}

fn spawn_keepalive(args: Args, sock: Arc<UdpSocket>) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("fake-cdj-keepalive".into())
        .spawn(move || {
            let pkt = build_keepalive(&args);
            let mut n: u64 = 0;
            loop {
                if sock.send_to(&pkt, args.keepalive_target).is_ok() {
                    n += 1;
                    if n % 20 == 1 {
                        println!(
                            "keep-alive #{n} → {} ({} B)",
                            args.keepalive_target,
                            pkt.len()
                        );
                    }
                }
                thread::sleep(Duration::from_millis(1500));
            }
        })
        .expect("spawn keep-alive thread")
}

fn spawn_status(
    args: Args,
    sock: Arc<UdpSocket>,
    state: Arc<CdjState>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("fake-cdj-status".into())
        .spawn(move || {
            let mut n: u64 = 0;
            loop {
                let pkt = build_status(&args, &state);
                if sock.send_to(&pkt, args.status_target).is_ok() {
                    n += 1;
                    if n % 50 == 1 {
                        println!(
                            "status #{n} → {} ({} B, bpm={}, track={}, beat={})",
                            args.status_target,
                            pkt.len(),
                            args.bpm,
                            args.track_id,
                            state.beat_in_bar.load(Ordering::Relaxed)
                        );
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
        })
        .expect("spawn status thread")
}

fn spawn_beat(
    args: Args,
    sock: Arc<UdpSocket>,
    state: Arc<CdjState>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("fake-cdj-beat".into())
        .spawn(move || {
            let ms_per_beat = if args.bpm > 0.0 {
                (60_000.0 / args.bpm) as u64
            } else {
                500
            };
            let interval = Duration::from_millis(ms_per_beat);
            let mut n: u64 = 0;
            loop {
                // Advance beat-in-bar before emitting so the first
                // beat packet says "beat 1".
                let prev = state.beat_in_bar.load(Ordering::Relaxed);
                let next = if prev == 0 || prev >= 4 { 1 } else { prev + 1 };
                state.beat_in_bar.store(next, Ordering::Relaxed);
                state.beats_elapsed.fetch_add(1, Ordering::Relaxed);

                let pkt = build_beat(&args, &state);
                if sock.send_to(&pkt, args.beat_target).is_ok() {
                    n += 1;
                    if n % 16 == 1 {
                        println!(
                            "beat #{n} → {} ({} B, bb={})",
                            args.beat_target,
                            pkt.len(),
                            next
                        );
                    }
                }
                thread::sleep(interval);
            }
        })
        .expect("spawn beat thread")
}

fn main() {
    let args = Args::parse();
    println!("Pro DJ Link fake CDJ emitter starting");
    println!(
        "  device #{} name=\"{}\" bpm={} track_id={}",
        args.device_number, args.device_name, args.bpm, args.track_id
    );
    println!(
        "  keepalive → {}    status → {}    beat → {}",
        args.keepalive_target, args.status_target, args.beat_target
    );

    let sock = Arc::new(UdpSocket::bind("0.0.0.0:0").expect("bind src socket"));
    sock.set_broadcast(true).ok();

    let state = Arc::new(CdjState {
        beat_in_bar: AtomicU8::new(0),
        beats_elapsed: AtomicU32::new(0),
        status_packet_counter: AtomicU32::new(0),
    });

    let h1 = spawn_keepalive(args.clone(), sock.clone());
    let h2 = spawn_status(args.clone(), sock.clone(), state.clone());
    let h3 = spawn_beat(args.clone(), sock.clone(), state.clone());

    if args.duration_secs > 0 {
        let deadline = Instant::now() + Duration::from_secs(args.duration_secs);
        while Instant::now() < deadline {
            thread::sleep(Duration::from_millis(250));
        }
        println!("Duration {} s reached, exiting.", args.duration_secs);
        std::process::exit(0);
    }

    // Block forever — emitter threads run until process exit.
    let _ = h1.join();
    let _ = h2.join();
    let _ = h3.join();
}
