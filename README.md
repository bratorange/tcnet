# tcnet

Rust implementation of the [TCNet](https://www.tc-supply.com/tcnet) UDP
protocol used by professional DJ / VJ gear (Pioneer ProDJ-adjacent) for
synchronising playback state, mixer state, beat grids, waveforms and
timecode between networked nodes.

**Alpha.** Builds and passes its own test suite + manual smoke-tests
against PRO DJ LINK Bridge, but not yet exercised in any production
setting.  Expect breaking API changes between minor versions.

## What you get

A single typed handle, `Node<R: Role, V: SpecVersion>`, that hides the
wire encoding, socket I/O, peer discovery and session bookkeeping and
surfaces:

```rust
use tcnet::api::{NodeBuilder, Slave};
use tcnet::V3_6;

let mut node = NodeBuilder::<Slave, V3_6>::new()
    .with_local_ip([127, 0, 0, 1].into())
    .spawn()?;

let snap = node.snapshot();
for peer in &snap.peers {
    if peer.has_dj_controller {
        let layers = node.layers_for(peer.address).unwrap_or_default();
        // ... read state, request waveforms, etc.
    }
}
```

Swap `Slave` → `Master` to emulate a virtual CDJ:

```rust
use tcnet::api::{NodeBuilder, Master};
use tcnet::{V3_6, LayerId, Speed};

let mut node = NodeBuilder::<Master, V3_6>::new()
    .with_local_ip([127, 0, 0, 1].into())
    .spawn()?;

node.load_track(LayerId::L1, track_meta)?;
node.set_speed(LayerId::L1, Speed::NORMAL)?;
```

Compile-time role gating: a `Node<Slave, V3_6>` does not have
`broadcast_*` / `set_*` methods.  Compile-time spec-version gating:
methods introduced at later FLAMEs are only callable when
`V: IncludesFlame<F>`.

## What's implemented

Every spec-defined runtime behaviour:

- Discovery (OptIn broadcast 1 Hz + per-peer unicast; OptOut on shutdown;
  10 s peer-silence timeout)
- Time packet emission (broadcast 60001 every 20 ms + unicast to each
  discovered node)
- Status (broadcast 60000 every 1 s + unicast to slaves)
- Metrics / Meta / Mixer unicast at spec cadences
- TimeSync handshake with signed clock-offset computation per spec
  page 8
- Master election (1 Hz from observed `Master` / `Auto` peers; tie-break
  by uptime → announce time → node id)
- Request / Response for SmallWaveform, BigWaveform, BeatGrid, Cue
  Data, Artwork File (multi-packet reassembly built in;
  `ErrorNotification(014, EMPTY)` reply when cache is empty)
- AppSpecific (msg types 30 + 213) wire parsing

## Architecture

Lock-free internals: no `Mutex` / `RwLock` anywhere.  The peer map is an
`ArcSwap<HashMap<…>>`, per-peer state uses interior-mutable atomics and
`ArcSwap` fields, and outbound traffic flows over bounded `kanal`
channels drained by the dispatcher task.


## Running the example

```sh
cargo run --example observer -- 127.0.0.1
```

Pair with a PRO DJ LINK Bridge (or any TCNet-capable node) on the same
interface.

## Testing

```sh
cargo test
```

The integration tests under `src/tests.rs` bind the spec ports
`60000-60002 / 65023` on `127.0.0.1` — make sure no other TCNet node is
running on the same host before invoking the suite.

## Spec

V3.5.1B, vendored at [`docs/spec/TCNet-V3-5-1B.pdf`](docs/spec/TCNet-V3-5-1B.pdf).
Original source: <https://www.tc-supply.com/tcnet>.

## License

MIT OR Apache-2.0.
