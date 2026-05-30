//! In-process loopback [`Transport`] for tests.
//!
//! `MemoryTransport` plugs into the same `Transport` slot as
//! `UdpTransport` but never touches a UDP socket — instead it has four
//! per-channel inboxes a test can `.inject(...)` into, and a single
//! outbox that captures everything `Transport::send` ever produced for
//! later inspection.
//!
//! Why a memory transport at all? The session-layer property tests in
//! phase 4 want to drive thousands of scripted peer interactions per
//! run; that's impractical against real UDP sockets (especially on a
//! laptop where another instance of luchs may be holding ports
//! 60000–60002 / 65023, as the project CLAUDE.md warns).  By
//! exercising the trait, we get full coverage of the session layer
//! without the port discipline.
//!
//! ## Fault injection
//!
//! `MemoryTransport::set_drop_filter` installs an arbitrary predicate
//! over outgoing datagrams.  Returning `true` drops the datagram
//! silently and bumps `ChannelStatus::dropped`.  Phase 9's `pcap`
//! replay harness uses this to simulate scripted loss.

use super::channel::{Channel, ChannelConfig, ChannelStatus};
use super::{IncomingDatagram, Transport, TransportError};
use arc_swap::ArcSwap;
use crossbeam_queue::ArrayQueue;
use std::net::SocketAddrV4;
use std::sync::Arc;

/// One in-process datagram captured for inspection or injection.
#[derive(Debug, Clone)]
pub struct MemoryDatagram {
    /// The channel the datagram travelled on.
    pub channel: Channel,
    /// Sender address (for sent datagrams: the local addr of the
    /// `MemoryTransport`; for injected datagrams: whatever the test
    /// passed in).
    pub src: SocketAddrV4,
    /// Destination address (only meaningful on sent datagrams).
    pub dest: SocketAddrV4,
    /// Payload bytes.
    pub bytes: Vec<u8>,
}

type DropFilter = Arc<dyn Fn(&MemoryDatagram) -> bool + Send + Sync>;

/// In-process loopback transport.
///
/// Construct with [`MemoryTransport::new`], plug into a
/// `dyn Transport`, and drive from a test:
/// - [`MemoryTransport::inject`] to deliver an inbound datagram.
/// - [`MemoryTransport::drain_sent`] to read what the code under test
///   put on the wire.
/// - [`MemoryTransport::set_drop_filter`] to script outbound loss.
#[derive(Clone)]
pub struct MemoryTransport {
    inner: Arc<MemoryInner>,
}

struct MemoryInner {
    local_addr: SocketAddrV4,
    configs: [ChannelConfig; 4],
    inboxes: [Arc<ArrayQueue<MemoryDatagram>>; 4],
    outbox: Arc<ArrayQueue<MemoryDatagram>>,
    status: [Arc<ArcSwap<ChannelStatus>>; 4],
    drop_filter: arc_swap::ArcSwapOption<DropFilterHolder>,
}

// Wrap the DropFilter Arc in a holder so ArcSwapOption can store it
// (ArcSwapOption requires a single Arc<T>).
struct DropFilterHolder(DropFilter);

impl MemoryTransport {
    /// Build a new transport bound to `local_addr` with the given
    /// per-channel configs.
    pub fn new(local_addr: SocketAddrV4, configs: [ChannelConfig; 4]) -> Self {
        // Use ArrayQueue capacities derived from configs.
        let inboxes: [Arc<ArrayQueue<MemoryDatagram>>; 4] = [
            Arc::new(ArrayQueue::new(configs[0].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[1].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[2].capacity.max(1))),
            Arc::new(ArrayQueue::new(configs[3].capacity.max(1))),
        ];
        // Outbox is one big queue across all channels (sized to sum
        // of per-channel capacities so a burst on any one channel
        // can't lose tail).
        let outbox_cap = configs
            .iter()
            .map(|c| c.capacity)
            .sum::<usize>()
            .max(1);
        let outbox = Arc::new(ArrayQueue::new(outbox_cap));
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
        Self {
            inner: Arc::new(MemoryInner {
                local_addr,
                configs,
                inboxes,
                outbox,
                status,
                drop_filter: arc_swap::ArcSwapOption::empty(),
            }),
        }
    }

    /// Bound address (the addr stamped on outgoing datagrams as
    /// `src`).
    pub fn local_addr(&self) -> SocketAddrV4 {
        self.inner.local_addr
    }

    /// Push a synthetic datagram into the local recv inbox for
    /// `channel`. Returns `true` if accepted, `false` if the inbox is
    /// at capacity (the test should turn down its rate or raise the
    /// channel config's capacity).
    pub fn inject(&self, channel: Channel, src: SocketAddrV4, bytes: Vec<u8>) -> bool {
        let dg = MemoryDatagram {
            channel,
            src,
            dest: self.inner.local_addr,
            bytes,
        };
        self.inner.inboxes[ch_idx(channel)].push(dg).is_ok()
    }

    /// Drain everything previously sent for inspection.  Returns in
    /// FIFO order.
    pub fn drain_sent(&self) -> Vec<MemoryDatagram> {
        let mut out = Vec::new();
        while let Some(dg) = self.inner.outbox.pop() {
            out.push(dg);
        }
        out
    }

    /// Install a predicate over outbound datagrams.  Returning `true`
    /// drops the datagram silently (no error, but
    /// `ChannelStatus::dropped` increments).
    pub fn set_drop_filter<F>(&self, f: F)
    where
        F: Fn(&MemoryDatagram) -> bool + Send + Sync + 'static,
    {
        let arc: DropFilter = Arc::new(f);
        self.inner
            .drop_filter
            .store(Some(Arc::new(DropFilterHolder(arc))));
    }

    /// Remove a previously-installed drop filter.
    pub fn clear_drop_filter(&self) {
        self.inner.drop_filter.store(None);
    }

    fn bump_processed(&self, channel: Channel) {
        let idx = ch_idx(channel);
        let cur = self.inner.status[idx].load_full();
        let mut next = (*cur).clone();
        next.processed = next.processed.saturating_add(1);
        next.queue_len = self.queue_len_for(channel);
        self.inner.status[idx].store(Arc::new(next));
    }

    fn bump_dropped(&self, channel: Channel) {
        let idx = ch_idx(channel);
        let cur = self.inner.status[idx].load_full();
        let mut next = (*cur).clone();
        next.dropped = next.dropped.saturating_add(1);
        next.queue_len = self.queue_len_for(channel);
        self.inner.status[idx].store(Arc::new(next));
    }

    fn queue_len_for(&self, channel: Channel) -> usize {
        self.inner.inboxes[ch_idx(channel)].len()
    }
}

impl Transport for MemoryTransport {
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
            src: self.inner.local_addr,
            dest,
            bytes: bytes.to_vec(),
        };

        // Check the drop filter.
        if let Some(holder) = self.inner.drop_filter.load_full() {
            if (holder.0)(&dg) {
                self.bump_dropped(channel);
                return Ok(());
            }
        }

        // Push to the outbox.  Outbox cap is the sum of per-channel
        // caps, so this only fills if every channel is at cap with
        // sent-but-undrained datagrams.
        if self.inner.outbox.push(dg).is_err() {
            self.bump_dropped(channel);
        } else {
            self.bump_processed(channel);
        }
        Ok(())
    }

    fn try_recv<'b>(&self, buf: &'b mut [u8]) -> Option<IncomingDatagram<'b>> {
        // Try each channel in spec order until we find a datagram.
        for channel in Channel::all() {
            if let Some(dg) = self.inner.inboxes[ch_idx(channel)].pop() {
                let n = dg.bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&dg.bytes[..n]);
                self.bump_processed(channel);
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
        s.queue_len = self.queue_len_for(channel);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn addr(last: u8) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, last), 60000)
    }

    fn default_cfgs() -> [ChannelConfig; 4] {
        Channel::all().map(ChannelConfig::default_for)
    }

    #[test]
    fn send_lands_in_outbox() {
        let t = MemoryTransport::new(addr(1), default_cfgs());
        t.send(Channel::Broadcast60000, addr(2), b"hello").unwrap();
        let sent = t.drain_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].channel, Channel::Broadcast60000);
        assert_eq!(sent[0].src, addr(1));
        assert_eq!(sent[0].dest, addr(2));
        assert_eq!(sent[0].bytes, b"hello");
    }

    #[test]
    fn send_oversized_payload_errors() {
        let t = MemoryTransport::new(addr(1), default_cfgs());
        let big = vec![0u8; super::super::pool::SLOT_SIZE + 1];
        let err = t.send(Channel::Unicast, addr(2), &big).unwrap_err();
        assert!(matches!(err, TransportError::PayloadTooLarge { .. }));
    }

    #[test]
    fn inject_and_try_recv_round_trips() {
        let t = MemoryTransport::new(addr(1), default_cfgs());
        assert!(t.inject(Channel::Time60001, addr(2), vec![0xAB, 0xCD]));
        let mut buf = [0u8; 64];
        let dg = t.try_recv(&mut buf).expect("recv");
        assert_eq!(dg.channel, Channel::Time60001);
        assert_eq!(dg.src, addr(2));
        assert_eq!(dg.bytes, &[0xAB, 0xCD]);
    }

    #[test]
    fn try_recv_returns_none_when_empty() {
        let t = MemoryTransport::new(addr(1), default_cfgs());
        let mut buf = [0u8; 8];
        assert!(t.try_recv(&mut buf).is_none());
    }

    #[test]
    fn try_recv_polls_channels_in_spec_order() {
        let t = MemoryTransport::new(addr(1), default_cfgs());
        // Inject one packet on each channel; pop should return them in
        // spec order: 60000, 60001, 60002, 65023.
        t.inject(Channel::Unicast, addr(9), vec![3]);
        t.inject(Channel::Broadcast60002, addr(8), vec![2]);
        t.inject(Channel::Time60001, addr(7), vec![1]);
        t.inject(Channel::Broadcast60000, addr(6), vec![0]);

        let mut buf = [0u8; 8];
        let order: Vec<Channel> = (0..4)
            .map(|_| t.try_recv(&mut buf).unwrap().channel)
            .collect();
        assert_eq!(order, Channel::all().to_vec());
    }

    #[test]
    fn drop_filter_skips_send_and_bumps_dropped() {
        let t = MemoryTransport::new(addr(1), default_cfgs());
        t.set_drop_filter(|dg| dg.channel == Channel::Time60001);
        t.send(Channel::Time60001, addr(2), b"a").unwrap();
        t.send(Channel::Broadcast60000, addr(2), b"b").unwrap();

        let sent = t.drain_sent();
        assert_eq!(sent.len(), 1, "Time60001 dropped, Broadcast60000 kept");
        assert_eq!(sent[0].channel, Channel::Broadcast60000);
        assert_eq!(t.channel_status(Channel::Time60001).dropped, 1);
        assert_eq!(t.channel_status(Channel::Broadcast60000).dropped, 0);
    }

    #[test]
    fn channel_status_reports_processed_count() {
        let t = MemoryTransport::new(addr(1), default_cfgs());
        for _ in 0..3 {
            t.send(Channel::Broadcast60000, addr(2), b"x").unwrap();
        }
        assert_eq!(
            t.channel_status(Channel::Broadcast60000).processed,
            3
        );
    }

    #[test]
    fn channel_status_reports_inbox_queue_len() {
        let t = MemoryTransport::new(addr(1), default_cfgs());
        t.inject(Channel::Time60001, addr(2), vec![0]);
        t.inject(Channel::Time60001, addr(2), vec![1]);
        assert_eq!(t.channel_status(Channel::Time60001).queue_len, 2);
    }

    #[test]
    fn memory_transport_is_send_sync() {
        fn _ss<T: Send + Sync>() {}
        _ss::<MemoryTransport>();
    }
}
