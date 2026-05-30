//! Real-network [`Transport`] backed by tokio [`UdpSocket`]s.
//!
//! `UdpTransport` binds the four spec-mandated UDP sockets (or
//! caller-overridden ports for tests) and shuttles datagrams between
//! the wire and the in-process queues defined by [`Channel`]:
//!
//! ```text
//!     ╔═══════════════════════ UdpTransport ═══════════════════════╗
//!     ║                                                            ║
//!     ║  Socket 60000  ──── recv task ──► inbox[Broadcast60000] ──►║─► try_recv
//!     ║  Socket 60001  ──── recv task ──► inbox[Time60001]      ──►║
//!     ║  Socket 60002  ──── recv task ──► inbox[Broadcast60002] ──►║
//!     ║  Socket 65023  ──── recv task ──► inbox[Unicast]        ──►║
//!     ║                                                            ║
//!     ║  send  ───► outbox[channel] ──── send task ──► sendto(2)   ║
//!     ║                                                            ║
//!     ╚════════════════════════════════════════════════════════════╝
//! ```
//!
//! Phase 7 replaces the recv / send tokio tasks with three dedicated
//! `std::thread`s using `clock_nanosleep` ticking; the `Transport`
//! trait stays unchanged.

use super::channel::{Channel, ChannelConfig, ChannelStatus};
use super::memory::MemoryDatagram;
use super::{IncomingDatagram, Transport, TransportError};
use arc_swap::ArcSwap;
use crossbeam_queue::ArrayQueue;
use log::{trace, warn};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::UdpSocket;
use tokio::sync::Notify;

/// The four spec ports — 60000 / 60001 / 60002 / 65023.
///
/// Pass to [`UdpTransport::bind`] for real-network usage; tests pass
/// `[0; 4]` so the kernel picks ephemeral ports.
pub const DEFAULT_CHANNEL_PORTS: [u16; 4] = [60000, 60001, 60002, 65023];

/// Real-UDP [`Transport`] impl.
#[derive(Clone)]
pub struct UdpTransport {
    inner: Arc<UdpInner>,
}

struct UdpInner {
    local_ip: Ipv4Addr,
    /// Actual ports bound (after kernel resolution if any was `0`).
    bound_ports: [u16; 4],
    configs: [ChannelConfig; 4],

    inboxes: [Arc<ArrayQueue<MemoryDatagram>>; 4],
    outboxes: [Arc<ArrayQueue<MemoryDatagram>>; 4],
    status: [Arc<ArcSwap<ChannelStatus>>; 4],

    /// Wakes the send task when an outbox gets a new datagram.
    send_notify: Arc<Notify>,
    /// Tells background tasks to shut down on drop.
    shutdown: Arc<AtomicBool>,
}

impl UdpTransport {
    /// Bind `ports[i]` on `local_ip` for each `Channel`, spawn the
    /// recv / send tokio tasks, and return a ready-to-use transport.
    ///
    /// `ports = DEFAULT_CHANNEL_PORTS` is the spec setup. For tests
    /// pass `[0; 4]` and read the kernel-picked ports back from
    /// [`UdpTransport::bound_ports`].
    ///
    /// Must be called from within a tokio runtime.
    pub async fn bind(
        local_ip: Ipv4Addr,
        ports: [u16; 4],
        configs: [ChannelConfig; 4],
    ) -> Result<Self, TransportError> {
        let mut sockets: Vec<Arc<UdpSocket>> = Vec::with_capacity(4);
        let mut bound_ports = [0u16; 4];
        for i in 0..4 {
            let want = ports[i];
            let socket = UdpSocket::bind(SocketAddrV4::new(local_ip, want))
                .await
                .map_err(|source| TransportError::BindFailed {
                    port: want,
                    source,
                })?;
            // Mark the two broadcast channels as broadcast-capable.
            // 65023 (Unicast) and 60001 (Time) don't need it; the spec
            // sends OptIn / Status via 60000, Time via 60001, both as
            // *broadcasts*. We mark 60000 and 60002 as broadcast; 60001
            // we also set since some implementations broadcast time
            // packets too.
            if matches!(
                Channel::all()[i],
                Channel::Broadcast60000 | Channel::Broadcast60002 | Channel::Time60001
            ) {
                if let Err(e) = socket.set_broadcast(true) {
                    warn!("set_broadcast(true) failed on port {}: {}", want, e);
                }
            }
            bound_ports[i] = socket
                .local_addr()
                .map(|a| a.port())
                .unwrap_or(want);
            sockets.push(Arc::new(socket));
        }

        let inboxes: [Arc<ArrayQueue<MemoryDatagram>>; 4] = [
            Arc::new(ArrayQueue::new(configs[0].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[1].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[2].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[3].capacity.max(1))),
        ];
        let outboxes: [Arc<ArrayQueue<MemoryDatagram>>; 4] = [
            Arc::new(ArrayQueue::new(configs[0].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[1].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[2].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[3].capacity.max(1))),
        ];
        let status: [Arc<ArcSwap<ChannelStatus>>; 4] = [
            Arc::new(ArcSwap::from_pointee(ChannelStatus {
                queue_cap: configs[0].capacity,
                ..ChannelStatus::default()
            })),
            Arc::new(ArcSwap::from_pointee(ChannelStatus {
                queue_cap: configs[1].capacity,
                ..ChannelStatus::default()
            })),
            Arc::new(ArcSwap::from_pointee(ChannelStatus {
                queue_cap: configs[2].capacity,
                ..ChannelStatus::default()
            })),
            Arc::new(ArcSwap::from_pointee(ChannelStatus {
                queue_cap: configs[3].capacity,
                ..ChannelStatus::default()
            })),
        ];

        let send_notify = Arc::new(Notify::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let inner = Arc::new(UdpInner {
            local_ip,
            bound_ports,
            configs,
            inboxes: inboxes.clone(),
            outboxes: outboxes.clone(),
            status: status.clone(),
            send_notify: send_notify.clone(),
            shutdown: shutdown.clone(),
        });

        // Spawn one recv task per socket.
        for (idx, socket) in sockets.iter().enumerate() {
            let socket = socket.clone();
            let inbox = inboxes[idx].clone();
            let status = status[idx].clone();
            let shutdown = shutdown.clone();
            let channel = Channel::all()[idx];
            tokio::spawn(async move {
                recv_loop(socket, inbox, status, shutdown, channel).await;
            });
        }

        // Spawn one send task fanning out to all four sockets.
        {
            let outboxes = outboxes.clone();
            let status = status.clone();
            let shutdown = shutdown.clone();
            let notify = send_notify.clone();
            tokio::spawn(async move {
                send_loop(sockets, outboxes, status, shutdown, notify).await;
            });
        }

        Ok(Self { inner })
    }

    /// Spec-port shorthand: `bind(local_ip, DEFAULT_CHANNEL_PORTS, …)`
    /// with default per-channel configs.
    pub async fn bind_default(local_ip: Ipv4Addr) -> Result<Self, TransportError> {
        let configs = Channel::all().map(ChannelConfig::default_for);
        Self::bind(local_ip, DEFAULT_CHANNEL_PORTS, configs).await
    }

    /// Actual ports bound (resolved from `0` to kernel-picked).
    pub fn bound_ports(&self) -> [u16; 4] {
        self.inner.bound_ports
    }

    /// Local IP all four sockets are bound to.
    pub fn local_ip(&self) -> Ipv4Addr {
        self.inner.local_ip
    }

    fn bump_status<F: FnOnce(&mut ChannelStatus)>(&self, channel: Channel, f: F) {
        let idx = ch_idx(channel);
        let cur = self.inner.status[idx].load_full();
        let mut next = (*cur).clone();
        f(&mut next);
        next.queue_len = self.inner.inboxes[idx].len();
        self.inner.status[idx].store(Arc::new(next));
    }
}

impl Drop for UdpTransport {
    fn drop(&mut self) {
        // Final Arc dropping — signal shutdown so background tasks
        // can wind down cleanly.
        if Arc::strong_count(&self.inner) == 1 {
            self.inner.shutdown.store(true, Ordering::Release);
            self.inner.send_notify.notify_waiters();
        }
    }
}

impl Transport for UdpTransport {
    type Error = TransportError;

    fn send(
        &self,
        channel: Channel,
        dest: SocketAddrV4,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        if bytes.len() > super::pool::SLOT_SIZE {
            return Err(TransportError::PayloadTooLarge {
                len: bytes.len(),
                max: super::pool::SLOT_SIZE,
            });
        }
        let dg = MemoryDatagram {
            channel,
            src: SocketAddrV4::new(self.inner.local_ip, self.inner.bound_ports[ch_idx(channel)]),
            dest,
            bytes: bytes.to_vec(),
        };
        let idx = ch_idx(channel);
        if self.inner.outboxes[idx].push(dg).is_err() {
            self.bump_status(channel, |s| s.dropped = s.dropped.saturating_add(1));
        } else {
            // Wake the send task.
            self.inner.send_notify.notify_one();
        }
        Ok(())
    }

    fn try_recv<'b>(&self, buf: &'b mut [u8]) -> Option<IncomingDatagram<'b>> {
        for channel in Channel::all() {
            if let Some(dg) = self.inner.inboxes[ch_idx(channel)].pop() {
                let n = dg.bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&dg.bytes[..n]);
                return Some(IncomingDatagram {
                    bytes: &buf[..n],
                    src: dg.src,
                    channel,
                });
            }
        }
        None
    }

    fn channel_status(&self, channel: Channel) -> ChannelStatus {
        let idx = ch_idx(channel);
        let cur = self.inner.status[idx].load_full();
        let mut s = (*cur).clone();
        s.queue_len = self.inner.inboxes[idx].len();
        s.queue_cap = self.inner.configs[idx].capacity;
        s
    }
}

const fn ch_idx(c: Channel) -> usize {
    match c {
        Channel::Broadcast60000 => 0,
        Channel::Time60001 => 1,
        Channel::Broadcast60002 => 2,
        Channel::Unicast => 3,
    }
}

async fn recv_loop(
    socket: Arc<UdpSocket>,
    inbox: Arc<ArrayQueue<MemoryDatagram>>,
    status: Arc<ArcSwap<ChannelStatus>>,
    shutdown: Arc<AtomicBool>,
    channel: Channel,
) {
    let mut buf = [0u8; super::pool::SLOT_SIZE];
    loop {
        if shutdown.load(Ordering::Acquire) {
            return;
        }
        match tokio::time::timeout(
            std::time::Duration::from_millis(100),
            socket.recv_from(&mut buf),
        )
        .await
        {
            Ok(Ok((size, std::net::SocketAddr::V4(src)))) => {
                trace!(
                    "UdpTransport recv {} bytes on {:?} from {}",
                    size, channel, src
                );
                let dg = MemoryDatagram {
                    channel,
                    src,
                    dest: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0),
                    bytes: buf[..size].to_vec(),
                };
                let push_result = inbox.push(dg);
                let cur = status.load_full();
                let mut next = (*cur).clone();
                if push_result.is_err() {
                    next.dropped = next.dropped.saturating_add(1);
                } else {
                    next.processed = next.processed.saturating_add(1);
                }
                next.queue_len = inbox.len();
                status.store(Arc::new(next));
            }
            Ok(Ok((_, std::net::SocketAddr::V6(_)))) => {
                // TCNet is IPv4-only per spec page 7.  Drop silently.
            }
            Ok(Err(e)) => {
                warn!("UdpTransport recv error on {:?}: {}", channel, e);
                // Brief backoff so we don't spin on EBADF.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(_elapsed) => { /* timeout — loop and re-check shutdown */ }
        }
    }
}

async fn send_loop(
    sockets: Vec<Arc<UdpSocket>>,
    outboxes: [Arc<ArrayQueue<MemoryDatagram>>; 4],
    status: [Arc<ArcSwap<ChannelStatus>>; 4],
    shutdown: Arc<AtomicBool>,
    notify: Arc<Notify>,
) {
    loop {
        if shutdown.load(Ordering::Acquire) {
            return;
        }
        // Drain each channel's outbox round-robin.
        let mut sent_anything = false;
        for (idx, outbox) in outboxes.iter().enumerate() {
            while let Some(dg) = outbox.pop() {
                sent_anything = true;
                match sockets[idx].send_to(&dg.bytes, dg.dest).await {
                    Ok(_) => {
                        let cur = status[idx].load_full();
                        let mut next = (*cur).clone();
                        next.processed = next.processed.saturating_add(1);
                        status[idx].store(Arc::new(next));
                    }
                    Err(e) => {
                        warn!(
                            "UdpTransport send to {} on {:?} failed: {}",
                            dg.dest,
                            Channel::all()[idx],
                            e
                        );
                        let cur = status[idx].load_full();
                        let mut next = (*cur).clone();
                        next.dropped = next.dropped.saturating_add(1);
                        status[idx].store(Arc::new(next));
                    }
                }
            }
        }
        if !sent_anything {
            // No work — wait for a notify or shutdown.
            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ephemeral_configs() -> [ChannelConfig; 4] {
        Channel::all().map(ChannelConfig::default_for)
    }

    /// Loopback round-trip: bind two transports on ephemeral ports,
    /// have one send to the other's `Unicast` port, assert the bytes
    /// land via `try_recv`.
    #[tokio::test]
    async fn udp_transport_round_trip_on_ephemeral_ports() {
        let a = UdpTransport::bind(
            Ipv4Addr::LOCALHOST,
            [0, 0, 0, 0],
            ephemeral_configs(),
        )
        .await
        .expect("bind a");
        let b = UdpTransport::bind(
            Ipv4Addr::LOCALHOST,
            [0, 0, 0, 0],
            ephemeral_configs(),
        )
        .await
        .expect("bind b");

        let b_unicast_port = b.bound_ports()[3];
        let dest = SocketAddrV4::new(Ipv4Addr::LOCALHOST, b_unicast_port);
        a.send(Channel::Unicast, dest, b"hello-udp").unwrap();

        // Poll up to ~500 ms for the packet to land.
        let mut buf = [0u8; 64];
        let mut got = None;
        for _ in 0..50 {
            if let Some(dg) = b.try_recv(&mut buf) {
                got = Some((dg.channel, dg.bytes.to_vec()));
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let (channel, bytes) = got.expect("packet should arrive");
        assert_eq!(channel, Channel::Unicast);
        assert_eq!(bytes, b"hello-udp");
    }

    #[tokio::test]
    async fn udp_transport_send_oversized_payload_errors() {
        let t = UdpTransport::bind(
            Ipv4Addr::LOCALHOST,
            [0, 0, 0, 0],
            ephemeral_configs(),
        )
        .await
        .expect("bind");
        let dest = SocketAddrV4::new(Ipv4Addr::LOCALHOST, t.bound_ports()[3]);
        let big = vec![0u8; super::super::pool::SLOT_SIZE + 1];
        let err = t.send(Channel::Unicast, dest, &big).unwrap_err();
        assert!(matches!(err, TransportError::PayloadTooLarge { .. }));
    }

    #[tokio::test]
    async fn udp_transport_bound_ports_resolved_from_zero() {
        let t = UdpTransport::bind(
            Ipv4Addr::LOCALHOST,
            [0, 0, 0, 0],
            ephemeral_configs(),
        )
        .await
        .expect("bind");
        let ports = t.bound_ports();
        // All four should be non-zero kernel-assigned ports.
        for (i, p) in ports.iter().enumerate() {
            assert_ne!(*p, 0, "port[{i}] should be resolved by kernel");
        }
    }

    #[test]
    fn udp_transport_is_send_sync() {
        fn _ss<T: Send + Sync>() {}
        _ss::<UdpTransport>();
    }
}
