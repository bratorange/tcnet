//! Send a single TimeSync(step=0) to a chosen address and listen for a reply.
//!
//! Used to investigate cases where the peer announces a `node_listener_port`
//! in its OptIn that differs from the port it actually has bound. By default
//! we send to whatever `--target` says and listen on `--our-port` for a reply
//! up to `--timeout` seconds.
//!
//! ```sh
//! cargo run --release --example timesync_direct -- \
//!     --target 127.0.0.1:65526 --our-port 65023 --timeout 5
//! ```

use clap::Parser;
use deku::DekuContainerWrite;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};
use tcnet::ApplicationConfig;
use tcnet::protocol::TimeSyncData;

#[derive(Parser)]
#[command(about = "Raw TimeSync probe")]
struct Args {
    /// Send the step=0 to this address.
    #[arg(long)]
    target: SocketAddrV4,
    /// Bind locally on this port; the bridge will reply here (or to whatever
    /// port we put in `node_listener_port` — those two are the same).
    #[arg(long, default_value_t = 65023)]
    our_port: u16,
    /// Seconds to wait for a reply before giving up.
    #[arg(long, default_value_t = 5)]
    timeout: u64,
}

fn main() {
    let args = Args::parse();
    println!("Sending TimeSync(step=0) to {}", args.target);
    println!("Listening for reply on 127.0.0.1:{}", args.our_port);

    let our_sock = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, args.our_port))
        .expect("bind our socket");
    our_sock
        .set_read_timeout(Some(Duration::from_secs(args.timeout)))
        .expect("set read timeout");

    // Build a header. We borrow the management_header builder via the public
    // protocol module if available; otherwise we hand-build the 24-byte header.
    let now_us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_micros();
    let ts = TimeSyncData::new_initiate(now_us, args.our_port);
    let body_bytes = ts.to_bytes().expect("serialize TimeSync body");
    println!("body_bytes ({} B) = {:02x?}", body_bytes.len(), body_bytes);

    // Hand-rolled 24-byte ManagementHeader matching V3.6, msg_type=10, node_id=0xCAFE.
    let mut header = [0u8; 24];
    // node_id (u16 LE) = 0xCAFE
    header[0] = 0xFE;
    header[1] = 0xCA;
    // protocol_version_major / minor = 3.6
    header[2] = 3;
    header[3] = 6;
    // "TCN"
    header[4] = b'T';
    header[5] = b'C';
    header[6] = b'N';
    // message_type = 10 (TimeSync)
    header[7] = 10;
    // node_name "PROBE000"
    let name = b"PROBE000";
    header[8..16].copy_from_slice(name);
    // seq = 1
    header[16] = 1;
    // node_type = 4 (Slave per protocol.rs enum mapping)
    header[17] = 4;
    // node_options (u16 LE) = 0
    header[18] = 0;
    header[19] = 0;
    // reserved
    header[20] = 0;
    // timestamp (u32 LE)
    header[21] = (now_us & 0xff) as u8;
    header[22] = ((now_us >> 8) & 0xff) as u8;
    header[23] = ((now_us >> 16) & 0xff) as u8;

    let mut packet = Vec::with_capacity(24 + body_bytes.len());
    packet.extend_from_slice(&header);
    packet.extend_from_slice(&body_bytes);
    println!("packet ({} B) = {:02x?}", packet.len(), &packet[..]);

    let target: SocketAddr = SocketAddr::V4(args.target);
    let sent_at = Instant::now();
    our_sock
        .send_to(&packet, target)
        .expect("send_to target");
    println!("sent at {:?}", sent_at);

    let mut buf = [0u8; 4096];
    match our_sock.recv_from(&mut buf) {
        Ok((size, src)) => {
            let elapsed = sent_at.elapsed();
            println!(
                "received {} bytes from {} after {:?}",
                size, src, elapsed
            );
            println!("payload = {:02x?}", &buf[..size.min(80)]);
        }
        Err(e) => {
            println!("no reply: {} (after {:?})", e, sent_at.elapsed());
        }
    }
    // Silence unused warning.
    let _ = ApplicationConfig::default();
}
