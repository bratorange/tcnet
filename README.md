**Alpha.** The library compiles, but it is **currently untested in any production setting**.
Expect rough edges, breaking API changes, and missing packet variants.

# tcnet

Rust implementation of the [TCNet protocol](https://www.tc-supply.com/tcnet).

Two roles:

- Passive observer — discover foreign DJ controllers and read their state via
  `DjControllerView`.
- Active broadcaster — emulate a TCNet node via `ActiveDJNode`, announce
  layers, push Time/Status/Metrics/Meta/Mixer packets, serve waveform /
  beat-grid / cue / artwork responses.

## Example observer

```sh
cargo run --example observer
```

## Test

```sh
cargo test -- --test-threads=1 --nocapture
```

## Documentation

API reference: <https://docs.rs/tcnet>

The full TCNet v3.6 spec is mirrored at
<https://www.tc-supply.com/_files/ugd/b1c714_0b351a4099c14e738f0cd7fcea623265.pdf>.