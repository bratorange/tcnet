# tcnet — architecture

> A real-time-safe, version-aware, layered Rust implementation of the
> TCNet UDP protocol.

---

## 0. Goals

1. **Real-time compatibility.** The hot path — Time emission (≥ 50 Hz),
   Metrics emission, inbound packet processing, snapshot publication —
   takes no locks, blocks on no syscalls, and performs no heap
   allocations. Latency from "bytes hit the kernel" to "snapshot is
   visible to a consumer" is bounded and predictable across the full
   load envelope of the spec.
2. **Spec compliance up to V3.5.1B**, with room to extend through
   future revisions. Every wire field, message type and protocol
   step in the official spec is representable. Unknown future fields
   are forward-compatible by construction.
3. **Multi-version coexistence.** Peers running different FLAME
   revisions (V3-1 through V3-6 today, V3-7+ tomorrow) coexist on the
   same network. The local node knows which features its peers
   support and refuses to use features they don't.
4. **Type-system enforcement of invariants.** Anything that can be a
   compile-time invariant is one. Run-time checks exist only for
   things the compiler cannot see (peer behaviour, wire-level data).
5. **Independent layer testability.** Every layer has a mock-able
   boundary. Wire round-trips, transport simulation, peer-state
   property tests, and protocol tabletops all run without UDP sockets.
6. **Pluggable transport.** UDP is the production backend; an
   in-memory backend exists for tests and for fixture-driven
   debugging.

---

## 1. Constraints

These shape everything that follows.

### 1.1 Real-time discipline

The hot path is everything that runs at, or in response to, the
50 Hz time-packet tick. Specifically:

| Activity | Hot? | Period / latency budget |
|---|---|---|
| Build outgoing Time packet | yes | ≤ 200 µs of the 20 ms window |
| Send Time packet via UDP | yes | ≤ 200 µs |
| Drain inbound socket queue | yes | bounded per tick |
| Parse inbound packet | yes | ≤ 100 µs / packet |
| Update snapshot | yes | ≤ 50 µs / packet |
| Build outgoing Metrics packet | yes | ≤ 200 µs / 50 ms window |
| Re-assemble multi-packet response | no | best-effort, off the RT thread |
| Decode artwork JPEG | no | off the RT thread |
| Build OptIn / OptOut | no | 1 Hz, off-tick |
| Vendor-specific Control or Text messages | no | user-driven, off the RT thread |

Hot-path code must:

- **Take no `Mutex` or `RwLock`.** Neither `std::sync::Mutex` nor
  `tokio::sync::Mutex`/`RwLock` ever appears in the source. Shared
  state is reached through wait-free or lock-free primitives (§10).
- **Allocate no heap memory.** Buffers are pre-allocated at startup
  and recycled. Outgoing packets are serialised into pre-sized
  `[u8; N]` scratch space.
- **Avoid blocking syscalls.** UDP I/O uses non-blocking sockets;
  reads are drained with a single batched receive per tick.
- **Avoid `Drop` that does work.** No `Drop` impl on a hot-path type
  performs network I/O or memory allocation.

Cold-path code (request/response handlers, reassembly, Control
messages) may allocate and may use bounded waits — but it runs on a
different thread, communicates with the RT thread via lock-free
queues, and never blocks the hot loop.

### 1.2 FLAME versioning

Every field in every TCNet packet body carries a FLAME tag in the
spec — the revision in which it was added. A peer claiming
`protocol_version = 3.6` is not obligated to send V3-6 fields if its
firmware was cut at V3-3-2; conversely, a future V3-7 peer may emit
trailing bytes we don't yet understand.

The wire layer (§3) treats FLAME as a first-class concept:

- Every body type knows which FLAME each of its fields was added in.
- Reads are *graceful in both directions*: short tails leave late
  fields as `Option::None`; long tails are captured into a
  `forward_compat_tail: Bytes` and preserved.
- Writes are produced at the local node's declared `SpecVersion` and
  no later.

The session layer (§5) attaches the negotiated `SpecVersion` to every
peer; the protocol layer (§6) refuses to use a feature past the
peer's declared FLAME.

### 1.3 Full V3.5.1B coverage as a design target

A partial implementation is acceptable as a *release schedule*, not
as an architectural assumption. Every part of the spec — including
the parts not yet implemented (TimeSync handshake, Control / Text /
Keyboard messages, Master election, authentication, V3-1 backwards
compat, future V3-7 forward compat) — has a defined home in the
architecture.

---

## 2. Layer overview

```
                            ┌──────────────────────────────────┐
                            │       Layer 6 — public API       │
                            │  Node<R: Role, V: SpecVersion>   │
                            │  (Slave / Master / Auto / Rptr)  │
                            │  async + blocking façades        │
                            └─────────────────┬────────────────┘
                                              │ typed commands (mpsc)
                                              │ typed snapshots (triple buf)
                            ┌─────────────────▼────────────────┐
                            │     Layer 5 — domain snapshots   │
                            │  LayerSnapshot / MixerSnapshot   │
                            │  MasterElectionState             │
                            │  ts-ordered cross-packet merge   │
                            └─────────────────┬────────────────┘
                                              │ committed deltas
                            ┌─────────────────▼────────────────┐
                            │    Layer 4 — protocol machines   │
                            │  Request / Response (chunked)    │
                            │  TimeSync handshake              │
                            │  Control / Text / Keyboard       │
                            │  AppSpecific (msgs 30 + 213)     │
                            │  ErrorNotification consumption   │
                            └─────────────────┬────────────────┘
                                              │ Wire-level events
                            ┌─────────────────▼────────────────┐
                            │     Layer 3 — session & peers    │
                            │  Peer lifecycle state machine    │
                            │  Sole owner of SEQ + uptime      │
                            │  Discovery, master election      │
                            │  Authentication handshake        │
                            └─────────────────┬────────────────┘
                                              │ addressed bytes
                            ┌─────────────────▼────────────────┐
                            │      Layer 2 — transport         │
                            │  UDP sockets (broadcast/unicast) │
                            │  Wait-free ring buffers          │
                            │  Pluggable: real / in-memory     │
                            └─────────────────┬────────────────┘
                                              │ raw datagrams
                            ┌─────────────────▼────────────────┐
                            │    Layer 1 — wire format (pure)  │
                            │  Total functions: bytes ↔ AST    │
                            │  FLAME-version-aware parsers     │
                            │  Zero alloc on the hot path      │
                            └──────────────────────────────────┘
```

Layer boundaries are *enforced*:

- The wire layer (L1) never opens a socket.
- The transport layer (L2) never decodes a packet.
- The session layer (L3) never serialises bytes — it asks the wire
  layer to do it.
- The protocol layer (L4) never owns a socket or a SEQ counter.
- The domain layer (L5) never speaks UDP.
- The API layer (L6) never builds a `ManagementHeader` directly.

Each layer is independently testable: an in-memory transport at L2
lets L3-L6 run without UDP; mock peer events at L3 let L4-L6 run
without a network; mock protocol traffic at L4 lets L5-L6 run without
peers; snapshot fixtures at L5 let L6 run with deterministic state.

---

## 3. Layer 1 — Wire format

**Job**: convert between `&[u8]` and a typed AST, *totally*.

**Inputs**: raw datagrams; a `SpecVersion` context.
**Outputs**: typed packet enums; or `WireError`.
**Side effects**: none.
**State**: none.
**Allocation budget**: zero on the read path (zero-copy parsing into
borrowed slices). Constructive write path uses a caller-supplied
scratch buffer.

### 3.1 Newtypes everywhere

Every field that has a domain constraint becomes a newtype with a
fallible constructor. The wire layer provides the constructors; the
deku-equivalent read impl calls them; callers cannot bypass them.

```rust
pub struct NodeId(u16);
pub struct Seq(u8);                          // wraps; only the session can mint
pub struct LayerIdx(u8);                     // 0..=7 enforced
pub struct Bpm(u32);                         // bpm × 100; >= 1
pub struct Speed(u32);                       // 0..=65_535; 32768 = 100%
pub struct Timestamp(u32);                   // 0..=999_999 µs per spec
pub struct Uptime(u16);                      // 0..=43_199 s per spec, 12 h roll
pub struct AsciiText<const N: usize>([u8; N]); // valid ASCII enforced on read

impl Bpm {
    pub fn try_from_f32(v: f32) -> Result<Self, BpmError> {
        if !v.is_finite() || v <= 0.0 { return Err(BpmError::NotPositive); }
        Ok(Bpm((v * 100.0).round() as u32))
    }
}
```

The only way to obtain a `Bpm`, `LayerIdx` or `Speed` is through a
constructor that has already checked the invariant. Whole categories
of "I passed a `u32` that meant the wrong thing" disappear at the
type level.

### 3.2 Total parsers: `Result`, never `panic`

Every read function returns `Result<T, WireError>`. `WireError` is a
flat enum with parse-position info; nested deku errors do not leak
into the public surface.

```rust
#[non_exhaustive]
pub enum WireError {
    Truncated   { want: usize, have: usize, at: &'static str },
    InvalidEnum { name: &'static str, value: u32 },
    InvalidUtf8 { at: &'static str },
    InvalidMagic{ at: &'static str, got: [u8; 3] },
    UnknownMessageType(u8),
}
```

Bit-set fields use a `Bits::Unknown(u16)` fallback variant for bits
the current spec does not name; receivers preserve them and emitters
never set them, so a forward-compatible peer's flags round-trip
losslessly.

A property-based round-trip test (`bytes → AST → bytes → AST`) on
every body guarantees writes match reads. A small fuzz target on the
read path catches `panic!` regressions before they ship.

### 3.3 FLAME versioning at field granularity

Each body knows which FLAME each of its fields was added in.

```rust
pub struct StatusBody {
    // V3-3 (present in every modern firmware)
    pub node_count:        u16,
    pub node_listener_port:u16,
    pub layer_source:      [u8; 8],
    pub layer_status:      [LayerStatus; 8],
    pub layer_track_id:    [u32; 8],
    pub smpte_mode:        SmpteMode,
    pub auto_master_mode:  AutoMasterMode,
    pub app_specific:      [u8; 72],
    // V3-3-2
    pub layer_name:        Option<[AsciiText<16>; 8]>,
    // any trailing bytes a future revision adds
    pub forward_compat_tail: Bytes,
}
```

The reader is parameterised by the *peer's* declared `SpecVersion`:

- If the peer says `>= V3-3-2`, `layer_name` is `Some(_)` or the read
  fails with `Truncated`.
- If the peer says `< V3-3-2`, `layer_name` is `None` and any
  remaining bytes go into `forward_compat_tail`.

For *outgoing* packets the local node has a fixed `SpecVersion`; the
write path is generic over a `SpecVersion` type parameter, so the
absence of a late field is a compile error rather than a runtime
"forgot to set it":

```rust
impl<V: IncludesFlame<LayerNameFlame>> StatusBuilder<V> {
    pub fn with_layer_names(self, names: [AsciiText<16>; 8]) -> Self { … }
}
```

A `V3_1` `StatusBuilder` doesn't have `with_layer_names`; a `V3_6`
`StatusBuilder` *must* call it because the field is non-optional in
the body it produces.

### 3.4 Provenance phantoms

Every wire frame carries its provenance as a phantom:

```rust
pub struct WireFrame<P: Provenance> {
    pub header: ManagementHeader,
    pub body:   WireBody,
    _provenance: PhantomData<P>,
}

pub trait Provenance: sealed::Sealed {}
pub struct Received;     // came from the network — SEQ / ts already set
pub struct Building;     // local builder, no SEQ / ts yet
pub struct Outgoing;     // built locally, SEQ / ts committed by Session
impl Provenance for Received {}
impl Provenance for Building {}
impl Provenance for Outgoing {}
```

A `WireFrame<Received>` cannot be passed to `Transport::send` — only
a `WireFrame<Outgoing>` can. The session layer (§5) is the only thing
that turns a `Building` into an `Outgoing`, because it is the only
holder of the SEQ counter and the uptime clock. Echoing an incoming
packet back unchanged is structurally impossible.

### 3.5 Allocation discipline

The read path borrows from the recv buffer:

```rust
pub fn parse<'a, V: SpecVersion>(
    bytes: &'a [u8],
    peer_version: V,
) -> Result<WireFrame<Received, 'a>, WireError>;
```

Multi-packet bodies (BeatGrid, BigWaveform, Artwork, AppSpecific
chunks) borrow the chunk payload from the source slice; the protocol
layer (§6) decides whether and when to copy into an owning buffer.
The write path takes a `&mut Cursor<&mut [u8]>` so the caller — not
the wire layer — decides where the bytes land.

### 3.6 Module shape

```
wire/
├── mod.rs           // re-exports + WireBody enum
├── header.rs        // ManagementHeader, SpecVersion, Flame trait
├── error.rs         // WireError
├── types.rs         // NodeId, Seq, LayerIdx, Bpm, Speed, …
├── opt.rs           // OptInBody, OptOutBody
├── status.rs        // StatusBody
├── time.rs          // TimePacketBody
├── time_sync.rs     // TimeSyncBody          (msg 10)
├── error_notif.rs   // ErrorNotificationBody (msg 13)
├── request.rs       // RequestBody           (msg 20)
├── control.rs       // ControlBody           (msg 101)
├── text.rs          // TextBody              (msg 128)
├── keyboard.rs      // KeyboardBody          (msg 132)
├── app_specific.rs  // AppSpecificBody       (msgs 30 + 213)
└── data/
    ├── mod.rs       // DataBody enum (msg 200 sub-types)
    ├── metrics.rs   // type 2
    ├── meta.rs      // type 4
    ├── beat_grid.rs // type 8
    ├── cue.rs       // type 12
    ├── waveform.rs  // type 16 + 32
    ├── artwork.rs   // type 128 (msg 204 file packet)
    └── mixer.rs     // type 150
```

The wire layer has no dependency on tokio, `Arc`, channels, sockets,
or thread-handling primitives. It compiles on `no_std + alloc`
(future-proof for embedded fixtures).

---

## 4. Layer 2 — Transport

**Job**: move bytes between the local node and `SocketAddrV4` peers,
with explicit per-channel back-pressure policy.

**Inputs**: outgoing addressed datagrams.
**Outputs**: incoming addressed datagrams + reception metadata.
**State**: the four bound UDP sockets (60000, 60001, 60002, unicast)
and the *actual* ports they bound to (after fallback).
**Allocation budget**: zero on the hot path. Buffer pools are
pre-allocated at startup.

### 4.1 Trait, two impls

```rust
pub trait Transport: Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;
    fn send(&self, channel: Channel, dest: SocketAddrV4, bytes: &[u8])
        -> Result<(), Self::Error>;
    fn try_recv<'b>(&self, buf: &'b mut [u8])
        -> Option<IncomingDatagram<'b>>;
    fn channel_status(&self, channel: Channel) -> ChannelStatus;
}

pub enum Channel {
    Broadcast60000,
    Time60001,
    Broadcast60002,
    Unicast,
}

pub struct IncomingDatagram<'b> {
    pub source:  SocketAddrV4,
    pub channel: Channel,
    pub bytes:   &'b [u8],
}
```

`try_recv` borrows from a caller-supplied buffer so the transport
never allocates. The wire layer parses out of the same borrow.

Two implementations ship:

- **`UdpTransport`** — non-blocking UDP sockets. Each socket is read
  with `recv_from` into a per-channel ring buffer, drained from the
  RT thread.
- **`MemoryTransport`** — deterministic in-memory bus for tests:
  programmable drop / reorder / fallback simulations, replay from a
  pcap.

### 4.2 Buffer pool

Send and receive paths share a single pool of fixed-size scratch
buffers, large enough to hold the biggest spec packet (currently a
~5 kB BigWaveform chunk; pool slots sized to the next power of two).

```rust
pub struct BufferPool { /* lock-free freelist of [u8; SLOT_SIZE] */ }
```

On the hot path the RT thread checks out a buffer, fills it, hands it
to the transport, and gets it back when the transport drops the
reference. Allocation is zero after warmup.

### 4.3 Bounded wait-free queues with stated policy

Outbound packets enqueue into per-channel wait-free SPSC ring buffers
sized at construction. Inbound datagrams enqueue into a single
wait-free MPSC ring drained by the session task.

```rust
pub struct ChannelConfig {
    pub capacity:  usize,
    pub overflow:  OverflowPolicy,
}
pub enum OverflowPolicy {
    DropOldest,                 // Time at 50 Hz prefers the newest sample
    DropNewest { warn: bool },  // RequestData responses
    BackPressureAsync,          // Cold-path Control / Text
}
```

Each channel's policy is part of its type at construction; the choice
is documented at the call site, not buried in the implementation.

### 4.4 No spec awareness

The transport layer is utterly oblivious to TCNet semantics — it
moves opaque bytes. `SpecVersion` sits one layer above; the transport
doesn't know that 60001 is the "Time" channel except as a name.

---

## 5. Layer 3 — Session

**Job**: maintain the set of known peers, the local node's
SEQ / uptime / timestamp counters, the discovery protocol, the
master-election arbitrator and the authentication handshake.

The session layer owns the bulk of node state and is the *sole*
writer to that state. Readers reach it through snapshots (§7).

### 5.1 The peer lifecycle as a state machine

A peer occupies exactly one of these states:

```
              OptIn         first DJ pkt        OptOut / 10 s silence
NotKnown ─────────► Announcing ──────────► Active ──────────────► Leaving
                       │                      │
                       │                 10 s silence
                       └─────────────────────►┴──► Gone
```

```rust
pub enum Peer {
    Announcing(PeerAnnouncing),  // OptIn received, no DJ traffic yet
    Active(PeerActive),          // DJ traffic flowing
    Leaving(PeerLeaving),        // OptOut received, drain in progress
}
```

Each variant exposes only the methods that make sense in its state:
`PeerAnnouncing` lets you read metadata; `PeerActive` adds layer and
mixer snapshots; `PeerLeaving` is read-only.

Transitions are functions of type `PeerAnnouncing -> PeerActive`,
`PeerActive -> PeerLeaving`, etc. — consuming the previous state. The
"DJ packet arrived before its OptIn" case is `PeerActive` constructed
from a `ManagementHeader` only; a later `OptIn` upgrades it
non-destructively.

### 5.2 SEQ counter — one owner, borrow-checked

```rust
pub struct SeqCounter(u8);
impl SeqCounter { pub fn next(&mut self) -> Seq { … } }
```

`LocalNode` owns the single `SeqCounter`. Anywhere a `WireFrame
<Outgoing>` is built, the construction signature requires `&mut
SeqCounter` — the borrow checker prevents a second source from
emitting packets. Two interleaved SEQ streams on the wire are
structurally impossible.

### 5.3 Uptime clock

```rust
pub struct UptimeClock { start: Instant }
impl UptimeClock {
    pub fn now(&self) -> Uptime {
        Uptime::new((self.start.elapsed().as_secs() % 43_200) as u16)
    }
}
```

A pure function of the monotonic clock; no shared state, no `Mutex`,
no field anybody can forget to advance.

### 5.4 Discovery (OptIn / OptOut)

- 1 Hz `OptIn` emission to every known broadcast destination and
  every discovered peer's listener port.
- On `OptIn` reception: insert/refresh peer in `Announcing` or
  `Active`.
- On `OptOut` reception: transition to `Leaving`, drain any pending
  replies, drop the peer after a short grace.
- On 10 s silence: synthetic `Leaving` with a `warn!` log.
- On local shutdown: a single OptOut broadcast goes out via
  `Node::leave(self)` (§8.4). If `leave()` was skipped the `Drop`
  impl makes a best-effort sync emission and logs a warning.

### 5.5 Master election (Auto role)

`NodeType ∈ {Auto, Master, Slave, Repeater}` is a static role for
Slave / Master / Repeater. `Auto` participates in an election when
the current master disappears:

```
Auto:
            Master alive
Watching ─────────► Watching
   │
   │ Master gone (OptOut or 10 s silence)
   ▼
Contending ───────► Elected(NodeId)
   │                     │
   │ losing tie-break    │ winning tie-break
   ▼                     ▼
Watching             becomes Master role
```

Tie-break: `(uptime, ts)` descending. The losing nodes return to
`Watching`; the winner publishes a typed `RoleEvent::ElectedMaster`
which the API layer (§8) translates into a typed role transition
`Node<Auto> → Node<Master>`.

### 5.6 Authentication

The NodeOptions flag `NEED_AUTHENTICATION` (bit 1) gates a future
handshake protocol. To leave room for it without paying type-system
cost up front:

```rust
pub enum PeerAuth { Anonymous, AuthRequired, Authenticated }
```

`PeerAuth::Authenticated` is the witness that gates privileged
methods (Control messages, vendor-specific writes). Peers that
have not completed the handshake stay `Anonymous`; only authenticated
peers expose the privileged API surface.

### 5.7 Bound to the wire layer

The session layer is the *only* component that creates
`WireFrame<Outgoing>`. It owns the SEQ counter, the uptime clock and
the `ManagementHeader`-fill logic. Outbound flow:

```
L4 (protocol)      L3 (session)          L1 (wire)         L2 (transport)
  build         →  attach SEQ + ts    →  serialise into  →  enqueue on
  WireFrame        produce Outgoing       scratch buffer    SPSC ring
  <Building>       <Outgoing>
```

---

## 6. Layer 4 — Protocol machines

**Job**: implement each non-trivial protocol of the spec as a small
state machine the session layer can hand work to.

### 6.1 The chunked-frame protocol

Several spec data types arrive in multi-packet form, all with the same
`(total_packets, packet_no, data_size, data_cluster_size)` header
shape:

- BeatGrid (msg 200/8)
- BigWaveform (msg 200/32)
- Artwork (msg 204/128)
- AppSpecific (msg 30 / 213) when payload > one cluster
- Future spec additions

A single generic captures all of them:

```rust
pub trait ChunkedPayload: Sized {
    type Assembled;
    fn layer_id(&self) -> LayerIdx;
    fn packet_no(&self) -> u32;
    fn total_packets(&self) -> u32;
    fn data_size(&self) -> u32;
    fn chunk_bytes(&self) -> &[u8];
    fn assemble(layer: LayerIdx, total: u32, bytes: Vec<u8>) -> Self::Assembled;
}

pub struct ChunkedFrame<T: ChunkedPayload> { /* bitset + buffer */ }
impl<T: ChunkedPayload> ChunkedFrame<T> {
    pub fn accept(&mut self, packet: T) -> AcceptOutcome<T::Assembled> { … }
}
```

Reassembly logic lives in one place. New chunked spec types inherit
it by implementing the trait. Hot-path activity is bounded by the
chunk count of the response, not the total payload size.

### 6.2 Request / Response

`RequestData` (msg 20) issues a categorical data request. The
response is either:

- a single data packet (Metrics, Meta, Cue, SmallWaveform, Mixer),
- a chunked sequence (BeatGrid, BigWaveform, Artwork), or
- an `ErrorNotification` (msg 13) with code 1 / 13 / 14 / 255.

The protocol layer presents all three as one typed `Pending<T>`
future with one typed error:

```rust
pub enum RequestError {
    Empty,        // code 014
    NotPossible,  // code 013
    Unknown,      // code 001
    Timeout,      // 5 s default
    PeerGone,     // peer transitioned to Leaving
}
```

When the local node's response cache hands back `None` for a request
addressed to us, the protocol layer emits an `ErrorNotification` —
the type signature offers no other completion path.

### 6.3 TimeSync handshake (msg 10)

The three-step protocol per spec page 8 is a tiny state machine:

```rust
pub struct PendingTimeSync { sent_at: Instant, our_ts: Timestamp }
impl PendingTimeSync {
    pub fn accept(self, reply: TimeSyncBody)
        -> Result<ClockOffset, TimeSyncError> { … }
}
```

The session layer runs one `PendingTimeSync` per peer at a time;
clock offsets become part of the peer's domain snapshot.

### 6.4 Control / Text / Keyboard

`Control` (msg 101) carries a string-typed "control path" like
`layer/1/state=6`. A typed builder covers the well-known forms:

```rust
ControlPath::set_layer_state(LayerIdx::L2, LayerState::Playing);
ControlPath::set_layer_source(LayerIdx::LA, SourceId::Layer1);
```

A `ControlPath::raw(&str)` escape hatch covers vendor-specific
commands. `Text` (128) and `Keyboard` (132) are tiny passthroughs
with the same builder pattern. All three protocols require an
`Authenticated` witness from the session layer (§5.6).

### 6.5 AppSpecific (msgs 30 + 213)

One parser handles both transport variants (30 = unicast / 60001
broadcast, 213 = 60000 broadcast). The chunked-frame machinery (§6.1)
reassembles multi-packet payloads. Public output:

```rust
pub struct AppSpecificFrame {
    pub vendor:   VendorId,    // registered code, spec p. 36
    pub payload:  Bytes,
}
```

Vendor codes are a const enum with `Unknown(u16)` fallback; the spec
table is reproduced verbatim in `wire/app_specific.rs`.

### 6.6 ErrorNotification consumption

Every protocol that issues a request listens for an
`ErrorNotification` matching its `(data_type, layer_id)` pair and
translates it into the relevant `*Error` variant.

---

## 7. Layer 5 — Domain snapshots

**Job**: present the protocol-layer events as decoded, lock-free,
read-only snapshots for consumers.

### 7.1 Snapshots, not state

Each consumer (a VJ UI, a waveform viewer, an analytics dashboard)
sees an immutable snapshot:

```rust
pub struct Snapshot {
    pub layers: [LayerSnapshot; 8],
    pub mixer:  MixerSnapshot,
    pub master: ElectionSnapshot,
    pub time:   TimecodeSnapshot,
}
```

The snapshot is written by the session main task (one writer) and
read by many consumers via `triple_buffer::Output<Snapshot>`. The
read path is wait-free: a consumer always sees the most recent
fully-committed snapshot, never a torn read, never a write that's
still in progress.

### 7.2 Cross-packet invariants

Snapshots are where cross-packet invariants are enforced. The session
task drains the inbound queue into a per-iteration vector, sorts by
`header.timestamp` and applies in order; out-of-order packets cannot
corrupt the snapshot.

- `LayerSnapshot.state` keeps the value observed with the latest
  `header.timestamp`.
- `LayerSnapshot.bpm`, `track_length_ms`, `total_time_ms` are
  `Option<…>` and only `Some` once a packet that carries them
  arrives.
- `MixerSnapshot.master_audio_level` is the *current* value, but the
  snapshot also carries the timestamp it was sampled at so consumers
  can flag staleness.

### 7.3 Versioned snapshots

The snapshot is the merged result of all packets observed, in their
highest-supported form. Each per-layer snapshot also carries the
*peer's* declared `SpecVersion`, so consumers that care about
firmware-dependent behaviour can branch on it.

### 7.4 Snapshot publication cadence

The session task commits a new snapshot when *any* relevant packet
arrives, throttled to one swap per 10 ms in the worst case to avoid
producer-side cache thrashing. The triple buffer ensures a consumer
reading at 60 Hz never sees a stale snapshot for more than one frame.

---

## 8. Layer 6 — Public API

**Job**: give the user a typed handle whose methods are gated by
role, spec version, and authentication state.

### 8.1 Role as a phantom

```rust
pub struct Node<R: Role, V: SpecVersion = V3_6> { … }

pub trait Role: sealed::Sealed {}
pub struct Slave;
pub struct Master;
pub struct Auto;
pub struct Repeater;
```

Methods are gated:

```rust
impl<V: SpecVersion> Node<Slave, V> {
    pub async fn request_big_waveform(&self, l: LayerIdx)
        -> Result<Waveform, RequestError>;
    pub fn snapshot(&self) -> &Snapshot;
    // no broadcast_time(): Slaves do not emit Time packets
}

impl<V: SpecVersion> Node<Master, V> {
    pub async fn broadcast_time(&self, t: TimePacket);
    pub async fn broadcast_status(&self, s: StatusPacket);
    // also has every Slave method
}
```

Trying to call `broadcast_time` on a `Node<Slave, V>` is a
compile-time error. There is no flag, runtime check or panic.

### 8.2 Capability tokens

Sensitive operations require a witness token returned by the
transition that proved the invariant:

```rust
impl PeerActive<Authenticated> {
    pub async fn send_control(&self, path: ControlPath)
        -> Result<(), RequestError>;
}
```

A peer that hasn't completed the authentication handshake doesn't
have the method.

### 8.3 Async + blocking façades

The default API is async (Tokio-compatible, but executor-agnostic).
A `tcnet-blocking` feature flag (or companion crate) wraps the async
surface in `block_on` for users who want a synchronous API. The crate
proper never calls `block_on` in user-visible code: blocking is the
caller's choice, not the library's.

### 8.4 No `Drop` magic

```rust
impl<R: Role, V: SpecVersion> Node<R, V> {
    pub async fn leave(self) -> Result<(), SessionError>;
}
```

`leave` consumes `self` and broadcasts a single OptOut before
releasing resources. The `Drop` impl is a best-effort sync fallback
with a `warn!` log so a forgotten `leave()` does not leave a corpse
on the network — but the type system *asks* the user to do the
right thing.

### 8.5 Sample shape

```rust
use tcnet::{Node, NodeConfig, V3_6, LayerIdx, Slave};

let cfg = NodeConfig::builder()
    .node_id(NodeId::new(0x42))
    .application(ascii!("myviz___________"))
    .build();

let node: Node<Slave, V3_6> = Node::join(cfg, runtime.handle()).await?;

let peer = node.discover().next_active().await?;
let waveform: Waveform = peer.request_big_waveform(LayerIdx::L1).await?;
let snapshot = peer.snapshot();          // wait-free

node.leave().await?;                     // explicit OptOut
```

A `Node<Master, V3_6>` additionally exposes the broadcaster methods.
A `Node<Auto, V3_6>` exposes a `wait_election()` future that returns
either a `Node<Master, V3_6>` (we won) or a `Node<Slave, V3_6>`
(we lost), consuming the original handle.

---

## 9. Cross-cutting — invariants and the bug classes they eliminate

The architecture makes whole categories of bug structurally
impossible. Each row names a pattern, where it lives, and the bug
class it rules out:

- **Total parsers.** Every wire-read function returns
  `Result<T, WireError>`; bitsets and enums fall back to
  `Unknown(_)`; the wire layer contains no `unwrap()` /
  `expect()` / `panic!()` (§3.2).
  → *Eliminates*: panics from malformed inbound packets, panics from
  forward-compat fields, denial-of-service from a single bad peer.

- **Newtypes with fallible constructors.** `Bpm`, `Speed`,
  `LayerIdx`, `NodeId`, `Seq`, `Uptime`, `AsciiText<N>`, `VendorId`
  (§3.1).
  → *Eliminates*: invalid domain values (NaN BPM, layer-id 99,
  non-ASCII in an ASCII field), cross-type confusion at the call
  site (a `u32` accidentally used as a layer index).

- **Provenance phantoms.** `WireFrame<Received | Building |
  Outgoing>` (§3.4).
  → *Eliminates*: echoing inbound packets back onto the wire,
  sending a packet that never had a SEQ committed, and reusing a
  decoded frame as an outbound one.

- **Single-owner state.** SEQ counter, uptime clock, peer table and
  snapshot writer owned by the session task; `&mut`-borrowed by
  outgoing-packet builders (§5.1, §5.2, §5.3).
  → *Eliminates*: divergent SEQ streams from multiple writers,
  forgotten uptime advances, peer-state divergence across tasks.

- **Sealed traits + role / state phantoms.** `Role<Slave | Master |
  Auto | Repeater>`, `PeerState<Announcing | Active | Leaving>`,
  `Auth<Anonymous | AuthRequired | Authenticated>` (§5.1, §5.6,
  §8.1).
  → *Eliminates*: a Slave broadcasting Time, a privileged call on a
  pre-OptIn peer, an unauthenticated Control message.

- **Witness tokens.** `Authenticated`, `Elected`, `MasterToken`
  returned by the transition that proved them, consumed by the
  privileged method (§5.5, §5.6).
  → *Eliminates*: privileged operations without proof of
  authorisation, master-only operations on a node that lost the
  election.

- **Linear consume-self transitions.** `Node::leave(self)`,
  `PendingTimeSync::accept(self, reply)`,
  `Auto::wait_election(self) -> Master | Slave`,
  `PeerAnnouncing -> PeerActive` (§5.1, §6.3, §8.4).
  → *Eliminates*: shutdown without OptOut, double-handshakes, stale
  role handles continuing to act after a transition.

- **`#[non_exhaustive]` on every user-visible enum / struct** (§3.2,
  §11).
  → *Eliminates*: breaking changes from spec extensions; new
  variants downstream require an explicit match arm.

- **`forward_compat_tail: Bytes` on every parsed body** (§12.5).
  → *Eliminates*: silent data loss when a peer sends a field from a
  later spec revision, silent corruption when a parser-implied size
  disagrees with the spec-stated size.

- **`IncludesFlame<F>` compile-time version gating.** A
  `Node<R, V3_3_2>` does not have `mixer_snapshot()`; a
  `Node<R, V3_5>` does (§12.3).
  → *Eliminates*: silently calling into a feature past the peer's
  declared FLAME, runtime "did this peer support it?" branches
  smeared across call sites.

- **Reassembly is the only public output of multi-packet
  protocols.** `ChunkedFrame<T>` returns `Self::Assembled`, never an
  individual chunk (§6.1).
  → *Eliminates*: silent truncation of BigWaveform / Artwork /
  BeatGrid / multi-packet AppSpecific responses, ad-hoc accumulators
  duplicated per request type.

- **Timestamp-ordered snapshot merge.** The snapshot writer drains
  the inbound queue, sorts by `header.timestamp`, and applies in
  order (§7.2).
  → *Eliminates*: an out-of-order stale Metrics packet clobbering a
  fresh Time packet's state, last-write-wins races between protocols
  that touch the same snapshot field.

- **Mutex-free concurrency.** Approved primitives only: SPSC rings,
  triple buffer, `ArcSwap`, atomics, parking; no `Mutex` / `RwLock`
  / `parking_lot` anywhere in the source, CI-enforced (§10.1).
  → *Eliminates*: lock contention, mutex poisoning, priority
  inversion in the audio / rendering thread, deadlock across actors,
  unpredictable tail latency from blocking on a lock.

- **Per-layer allocation budgets.** Hot path allocates nothing; cold
  path allocates only out of pre-sized buffer pools (§10.5).
  → *Eliminates*: allocator stalls on the RT thread, unbounded
  memory growth from inbound traffic, GC-style pauses.

- **Typed channel back-pressure.** Every queue is constructed with a
  `ChannelConfig` that names its capacity and overflow policy at the
  call site (§4.3).
  → *Eliminates*: unbounded queue growth under load, silent packet
  drops without a documented policy choice.

- **Cross-layer encapsulation.** The wire layer has no socket; the
  transport layer has no `SpecVersion`; the session layer has no
  serialiser; the domain layer has no UDP (§2).
  → *Eliminates*: ad-hoc shortcuts that bypass an invariant a layer
  guards, reach-through into another layer's private state,
  compile-time `use` of a type from outside its allowed cone.

---

## 10. Cross-cutting — concurrency & real-time discipline

### 10.1 No mutexes. None.

There is no `std::sync::Mutex`, no `std::sync::RwLock`, no
`tokio::sync::Mutex`, no `tokio::sync::RwLock`, no `parking_lot::*`
anywhere in the source.

This is a hard rule. The CI runs:

```bash
! rg --quiet 'sync::(Mutex|RwLock)|parking_lot' src/
```

and fails the build on a single match.

### 10.2 Approved primitives

| Use case | Primitive | Crate |
|---|---|---|
| Single-producer single-consumer queue | wait-free SPSC ring | `rtrb` |
| Multi-producer single-consumer queue | lock-free MPSC ring | `crossbeam-queue::ArrayQueue` |
| Single-writer many-readers snapshot | wait-free triple buffer | `triple_buffer` |
| Atomic config swap | `ArcSwap<Config>` (wait-free reads) | `arc-swap` |
| Counters / flags | `AtomicU{8,16,32,64}`, `AtomicBool` | std |
| One-shot completion signals | `oneshot::Receiver` (does not lock) | `tokio::sync::oneshot` |
| Periodic wakeups | timerfd / `clock_nanosleep` / pinned Tokio timer | std + `tokio::time` |
| Thread parking | `crossbeam::sync::Parker` | `crossbeam-utils` |

Where Tokio types appear (e.g. `oneshot`), they are the ones whose
implementations are documented to avoid blocking locks.

### 10.3 Threads & ownership

The runtime has four logical actors. Each owns its state outright;
none reads or writes another's state directly.

```
┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│  Recv thread     │    │ Session thread   │    │  Send thread     │
│  (RT priority)   │───▶│  (RT priority)   │───▶│  (RT priority)   │
│  drains 4 UDP    │    │  owns peer map,  │    │  drains 4 SPSC   │
│  sockets into    │    │  SEQ, uptime,    │    │  rings into UDP  │
│  one MPSC ring   │    │  snapshot writer │    │  send_to calls   │
└──────────────────┘    └──────────────────┘    └──────────────────┘
                                  │
                                  ▼
                        ┌──────────────────┐
                        │  Snapshot reader │
                        │  (any thread)    │
                        │  wait-free read  │
                        │  from triple buf │
                        └──────────────────┘
                                  │
                                  ▼
                        ┌──────────────────┐
                        │  Cold-path tasks │
                        │  (Tokio runtime) │
                        │  reassembly,     │
                        │  user requests   │
                        └──────────────────┘
```

- The **Recv thread** does a single non-blocking `recv_from` per
  socket per iteration into a borrowed scratch buffer, parses the
  wire-level header, and pushes an `IncomingDatagram` reference onto
  a wait-free MPSC ring.
- The **Session thread** drains the ring per tick, walks each frame
  through the peer state machine, updates per-peer protocol state,
  commits a new snapshot to the triple buffer, and emits outbound
  frames onto per-channel SPSC rings.
- The **Send thread** drains the outbound rings into UDP `send_to`
  calls. A failed `send_to` produces a `warn!` and is dropped.
- The **Snapshot readers** (the API layer, user code, the UI thread)
  read the triple buffer wait-free. They never touch session state.

Cold-path activity — reassembly of a multi-MB BigWaveform, parsing
artwork JPEGs, building Control messages — happens on a Tokio runtime
that talks to the session thread via SPSC rings. The Tokio runtime
may use its own (locked) machinery internally; the RT path does not
care because it never blocks on it.

### 10.4 Time-tick scheduling

The 50 Hz time tick is driven by:

- `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, …)` on Linux / macOS,
- `timeBeginPeriod(1) + QueryPerformanceCounter`-based polling on
  Windows.

Drift is corrected by computing the next tick from a wall-clock
anchor, not by adding 20 ms to the previous tick. The session task
checks tick-overrun (`now > deadline + 5 ms`) and emits a
`warn!("tick overrun: {} ms")` once per second of overrun time.

### 10.5 Allocation budget

| Layer | Hot-path alloc | Cold-path alloc |
|---|---|---|
| Wire (L1) | none — borrows | bounded — `Vec<u8>` for reassembly |
| Transport (L2) | none — buffer pool | bounded — initial pool size |
| Session (L3) | none — fixed peer table | bounded — peer-state transition |
| Protocol (L4) | none — pre-allocated state machines | yes — chunk buffers |
| Domain (L5) | none — `Copy` snapshot | none |
| API (L6) | n/a (cold path) | yes — user-facing futures |

A boot-time `BufferPool` of N buffers (default N=64, slot size 8 kB)
covers every spec packet. Peer state is a fixed-size array indexed
by a small hash of the peer's listener address; the maximum peer
count is configured at startup and not exceeded at runtime.

### 10.6 What the API user sees

A user calling `peer.snapshot()` on any thread gets a `&Snapshot`
back in O(1) wait-free time. A user awaiting
`peer.request_big_waveform(…)` is on the cold path; the future
completes when the protocol layer has finished reassembly.

The user's thread *can* be the audio thread or the rendering thread —
reading a snapshot is RT-safe.

---

## 11. Cross-cutting — error model

```rust
#[non_exhaustive]
pub enum TcnetError {
    Wire(WireError),
    Transport(TransportError),
    Session(SessionError),
    Protocol(ProtocolError),
}
```

Public API methods return one of these. `thiserror` derives provide
free `Display` impls. Every variant is `#[non_exhaustive]` so spec
extensions can grow the error surface non-breakingly.

Errors *never* panic. Library code contains zero `unwrap()`,
`expect()`, `panic!()` outside of `unreachable!()` arms whose
unreachability is proven by the type system.

---

## 12. Cross-cutting — spec versioning (FLAME)

### 12.1 `SpecVersion` trait

```rust
pub trait SpecVersion: sealed::Sealed {
    const MAJOR: u8;
    const MINOR: u8;
    const FLAMES: &'static [Flame];
}

pub struct V3_1;    impl SpecVersion for V3_1   { … }
pub struct V3_3_2;  impl SpecVersion for V3_3_2 { … }
pub struct V3_5;    impl SpecVersion for V3_5   { … }
pub struct V3_6;    impl SpecVersion for V3_6   { … }

/// Subtyping over FLAMEs: V3_5 supports everything V3_3_2 does.
pub trait IncludesFlame<F: Flame>: SpecVersion {}
```

### 12.2 Local version is fixed at construction

```rust
let node: Node<Slave, V3_6> = Node::join(cfg, …).await?;
```

The local node *always* sends in its declared version. Other peers
decide whether to accept based on their own versions.

### 12.3 Peer version is dynamic

Each peer carries a `peer.protocol_version() -> (u8, u8)`. Methods
that depend on a late-FLAME field are gated:

```rust
impl PeerActive {
    pub fn mixer_snapshot(&self) -> Option<MixerSnapshot> {
        // MixerData was added in V3-5
        if self.protocol < (3, 5) { return None; }
        Some(self.mixer.snapshot())
    }
}
```

or at compile time when the call site knows the version:

```rust
impl<V: IncludesFlame<MixerFlame>> Node<Slave, V> {
    pub fn mixer_snapshot(&self) -> MixerSnapshot { … }
}
```

### 12.4 Outgoing FLAME selection

The wire-layer builder for a `V3_3_2` local node does not include
mixer fields. A peer reading from us at V3_5 interprets the shorter
packet as "no mixer", not as malformed.

### 12.5 Forward-compat tails

Every parsed packet carries a `forward_compat_tail: Bytes`. A future
spec revision that adds a trailing field does not break existing
readers — old readers see `tail.len() > 0`, log
`trace!("V3-X+ tail seen from {peer}: {} B", tail.len())` once per
peer-version pair, and otherwise ignore it.

Spec ambiguities (e.g. a packet whose declared total size and
field-implied total size disagree) are resolved by treating the gap
as `forward_compat_tail`; the bytes are preserved verbatim for
post-hoc analysis.

### 12.6 Cross-version peer matrix

The integration suite runs every protocol-layer test against the
cartesian product:

```
local ∈ { V3_1, V3_3_2, V3_5, V3_6 }
remote ∈ { V3_1, V3_3_2, V3_5, V3_6 }
```

and asserts graceful degradation. A V3_6 local talking to a V3_1
peer sees `None` for the V3_3-and-later fields; a V3_1 local talking
to a V3_6 peer sees the V3-1-known fields and a non-empty
`forward_compat_tail`.

---

## 13. Implementation map — full V3.5.1B coverage

A row per spec feature, the layer that owns it, and the type-system
machinery it requires:

| Spec feature | Layer | Type-system support |
|---|---|---|
| ManagementHeader | Wire L1 | Newtypes for every field |
| OptIn / OptOut (msg 2, 3) | Session L3 | Discovery state machine |
| Status (msg 5) | Domain L5 | Snapshot writer |
| TimeSync (msg 10) | Protocol L4 | `PendingTimeSync` state machine |
| ErrorNotification (msg 13) | Protocol L4 | Typed `RequestError` |
| RequestData (msg 20) | Protocol L4 | Generic over request type |
| AppSpecific (msg 30 + 213) | Protocol L4 | Chunked frame + `VendorId` |
| Control (msg 101) | Protocol L4 | `ControlPath` typed builder + `Authenticated` |
| Text (msg 128) | Protocol L4 | Same as Control |
| Keyboard (msg 132) | Protocol L4 | Same as Control |
| Metrics (msg 200/2) | Domain L5 | Snapshot field with timestamp |
| Meta (msg 200/4) | Domain L5 | Snapshot field |
| BeatGrid (msg 200/8) | Protocol L4 | `ChunkedFrame<BeatGridChunk>` |
| Cue (msg 200/12) | Domain L5 | Snapshot field |
| SmallWaveform (msg 200/16) | Protocol L4 | Single-packet response |
| BigWaveform (msg 200/32) | Protocol L4 | `ChunkedFrame<BigWaveformChunk>` |
| Artwork (msg 204/128) | Protocol L4 | `ChunkedFrame<ArtworkChunk>` |
| Mixer (msg 200/150) | Domain L5 | `IncludesFlame<MixerFlame>` gate |
| TimePacket (msg 254) | Domain L5 + Master L6 | Snapshot writer + master broadcaster |
| Auto-master election | Session L3 | `Auto::wait_election()` transition |
| Authentication handshake | Session L3 | `Authenticated` witness |
| Repeater forwarding | Session L3 | Wire-level passthrough, no decode |
| Vendor codes | Wire L1 | const enum + `Unknown(u16)` |
| V3-1 backwards compat | Wire L1 | FLAME tags + `forward_compat_tail` |
| V3-7+ forward compat | Wire L1 | `forward_compat_tail` |

Every cell is either implemented or has a place to land. No spec
feature requires "a hole punched through the layers"; everything
fits the model.

---

## 14. Testing strategy

The test pyramid mirrors the layer stack.

1. **Wire layer (L1).** Round-trip property tests with `proptest`
   on every body type: `for any body B, parse(serialise(B)) == B`.
   `cargo-fuzz` target on the read path catches panics. Golden
   vectors captured from real CDJ-3000 firmware (one pcap per FLAME
   version) provide ground-truth alignment.
2. **Transport layer (L2).** Behaviour tests with the in-memory
   transport: send N, recv N, deterministic drop, deterministic
   reorder, simulated fallback. CI runs the suite without a network.
3. **Session layer (L3).** Property-based simulator: spawn N
   virtual nodes on `MemoryTransport`, assert peer state machines
   reach the right terminal states under arbitrary
   OptIn/OptOut/silence interleavings.
4. **Protocol layer (L4).** Each protocol module has a tabletop
   test: a scripted sequence of `(t, packet)` tuples, an asserted
   sequence of outputs. The chunked-frame generic gets a separate
   suite that covers reorder, duplicate-arrival and overflow cases.
5. **Domain layer (L5).** Snapshot consistency tests: given a stream
   of wire events, the snapshot at time T equals the expected merged
   state. Cross-packet invariants (timestamp ordering, late-arriving
   stale Metrics) live here.
6. **API layer (L6).** Doctests act as smoke tests. `trybuild`
   compile-fail tests pin the role / auth / version gates: "a
   `Node<Slave>` calling `broadcast_time()` must not compile" is
   a CI invariant.
7. **Real-time discipline.** A `cargo-rtcheck` step runs the hot
   path under `perf record`, asserts no mutex contention (no
   `__lll_lock_wait` symbols), and bounds the 99.9th-percentile
   per-tick latency under load. A CI grep ensures the source
   contains no `Mutex`, `RwLock` or `parking_lot` reference.
8. **Spec-version matrix.** Integration tests run the full
   protocol-layer suite against each `(local, remote)` SpecVersion
   pair (§12.6) to verify graceful degradation in both directions.

---

## 15. Non-goals

A few explicit non-goals:

- **Not a security boundary.** TCNet has no auth, no integrity check
  and no encryption at the wire level. A malicious node on the same
  L2 can do anything. The design hardens against *malformed* peers,
  not adversarial ones.
- **Not a multi-crate split.** Single crate, one module per layer.
  Easier to ship, easier to read, friendlier to `cargo doc`.
- **Not a "make every field public" pivot.** Fields stay private;
  newtype constructors are the only entry point.
- **Not a guarantee against firmware bugs in peer devices.** The
  forward-compat tail and the `Unknown(_)` variants in every enum
  mean that a misbehaving peer cannot crash us; they do not promise
  that a misbehaving peer's *data* is correct.
- **Not an `unsafe`-free crate.** The wait-free queues and atomic
  pointer swaps used in §10 contain `unsafe` blocks. These are
  audited per-line in the crates we depend on and confined to the
  primitives that need them. The tcnet source itself contains no
  `unsafe`.

---

## 16. Open questions

1. **Style A vs Style B for FLAME versioning** in §3.3 — runtime
   trailing-bytes vs compile-time generic. Recommendation: Style A
   for reads (peer versions are dynamic), Style B for writes
   (local version is fixed). Worth confirming against a prototype.
2. **Custom executor or Tokio for the cold path?** Tokio has the
   widest ecosystem, but its `sync` types contain locks. The hot
   path doesn't use them; the cold path can, with the understanding
   that cold-path latency is not bounded.
3. **Repeater role implementation.** A Repeater bridges two L2
   segments — does it run two transports, or one transport on two
   interfaces? The answer affects the Transport trait shape.
4. **Per-spec-version pcap fixtures.** Cheap to produce given access
   to each piece of hardware; otherwise the gap will be filled by
   round-trip property tests alone.
5. **No-std / embedded targets.** The wire layer compiles on
   `no_std + alloc` today. The transport and session layers depend on
   std `UdpSocket` and `Instant`. A future `no_std` build would need
   a `MonotonicClock` trait and an `embassy-net`-style transport.
