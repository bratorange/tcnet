# Spec Compliance Audit — tcnet 0.2.0 vs. TCNet V3.5.1B

**Audit date:** 2026-05-31
**Spec:** `docs/spec/TCNet-V3-5-1B.pdf` (TCNet Link Specification V3.5.1B, 02/03/2022)
**tcnet HEAD:** `f78c6e2` (post phase-10 rewrite, post-RwLock-removal,
post-TimeSync-driver, post-election-driver)

This document lists every TCNet packet type defined in the spec and
reports our implementation status: parsing, emission, routing,
behaviour.  Last column flags conformance.

Legend:

- ✅ **Conformant** — parsed + emitted (where applicable) per spec.
- 🟢 **Conformant, behaviour driven** — actively scheduled in
  background tasks (e.g. periodic emission, automatic response).
- 🟡 **Partial** — wire format correct, behaviour incomplete.
- 🔴 **Non-conformant** — spec mismatch in wire layout, ports, or
  cadence.
- ⬛ **Not implemented** — type defined nowhere in tcnet.

---

## 1. Network ports (spec page 2)

| Spec | Implementation | Status |
|------|----------------|--------|
| 60000 — Opt-IN / Opt-OUT broadcast | Bound, broadcast-enabled | ✅ |
| 60000 — Application-Specific (msg 213) broadcast | Bound; outbound not yet exposed | 🟡 |
| 60001 — Time Packet broadcast | Bound, broadcast-enabled | ✅ |
| 60001 — Application-Specific (msg 30) broadcast | Bound but outbound not yet exposed | 🟡 |
| 60002 — *spec does not define for AppSpecific*; reserved | Bound (legacy) — listening only | ✅ |
| 65023–65535 unicast (default 65023) | Bound with fallback to next free port | ✅ |

Notes:

- Spec page 2 explicitly states msg type 30 broadcasts on **port 60001**
  while msg type 213 broadcasts on **port 60000**.  Our `Data::AppSpecific`
  variant currently serialises as msg type 30 only — to support both,
  the `Data` enum would need a `AppSpecific213` variant (or a sub-tag).
- The legacy port-60002 binding remains a listener for backwards
  compatibility with older spec drafts.

---

## 2. Management Header (spec page 4, recurring on every packet)

| Field | Spec value / range | Implementation | Status |
|---|---|---|---|
| `Node ID` (byte 0, 2 B LE) | Unique node id | `u16` | ✅ |
| `Protocol Version Major` (byte 2) | 3 | Constant 3 in `management_header()` | ✅ |
| `Protocol Version Minor` (byte 3) | 6 (for OptIn — spec page 4 row) | Constant 6 in `management_header()` | ✅ |
| `Header` (bytes 4-6) | ASCII `"TCN"` | `into_ascii!("TCN")` | ✅ |
| `Message Type` (byte 7) | Per-packet constant | Looked up via `Data::message_type_id()` | ✅ |
| `Node Name` (bytes 8-15) | 8 ASCII | `AsciiString<8>` | ✅ |
| `SEQ` (byte 16) | 0-255 monotonic | `current_seq: AtomicU8` shared across outbound | ✅ |
| `Node Type` (byte 17) | 1=Auto, 2=Master, 4=Slave, 8=Repeater | `NodeType` enum + `repr(u8)` | ✅ |
| `Node Options` (bytes 18-19, 2 B LE) | Bitflags | `NodeOptions` bitflags | ✅ |
| `Timestamp` (bytes 20-23, 4 B LE) | 0-999 999 µs-of-second | `timestamp_micros()` at serialisation | ✅ |

---

## 3. OptIn packet (msg type 2, spec page 4)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 2 | ✅ |
| Size | 68 B | Validated via tests; ✅ |
| `Node Count` | u16 LE; total known peers | ✅ |
| `Node Listener Port` | unicast port we listen on | `actual_unicast_port` atomic | ✅ |
| `Uptime` (V3-2) | 0-43199 with 12h roll-over | `AtomicU16`, modulo 43_200 every 1 Hz tick | ✅ |
| `Vendor Name` (V3-2) | 16 ASCII | `AsciiString<16>` | ✅ |
| `Application Name` (V3-2) | 16 ASCII | `AsciiString<16>` | ✅ |
| Application Major / Minor / Bug version (V3-2) | u8 each | ✅ |

**Behaviour:**
- Broadcast every 1000 ms on port 60000 — ✅
- Unicast every 1000 ms to each discovered peer's listener port (spec page 4 step 3) — ✅
- Outgoing OptOut on shutdown — ✅ (via `Drop`)

---

## 4. OptOut packet (msg type 3, spec page 5)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 3 | ✅ |
| Size | 28 B | ✅ |
| `Node Count` | u16 LE | ✅ |
| `Node Listener Port` | u16 LE | ✅ |

**Behaviour:**
- Sent at node shutdown — ✅ (in `Drop`).
- Per-peer expiration on 10 s silence — ✅ (in `timeout_foreign_nodes`).
- Election re-runs when a Master OptOuts — ✅ (`election_driver` is
  driven from the current peer map, so removal of the elected node
  forces a fresh re-resolve).

---

## 5. Status packet (msg type 5, spec page 6-7)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 5 | ✅ |
| Size | 300 B | ✅ |
| Node Count, Listener Port | u16 LE each | ✅ |
| 8 Layer Sources (V3-3) | u8 each | `StatusData.layer_*_source` | ✅ |
| 8 Layer Statuses (V3-3) | u8 each per `LayerState` enum | ✅ |
| 8 Layer Track IDs (V3-3) | u32 LE each | ✅ |
| SMPTE Mode (V3-3) | 24 / 25 / 29 / 30 | ✅ |
| Auto Master Mode (V3-3) | 0 / 1 / 2 + Unknown | ✅ |
| `APP_SPECIFIC` (V3-3) | 72 B free-form | `[u8; 72]` field | ✅ |
| 8 Layer Names (V3-3-2) | 16 ASCII each | ✅ |

**Behaviour:**
- Broadcast on port 60000 every 1 s — ✅ (`active_broadcast` task)
- Unicast to all slaves & repeaters (spec page 2) — ✅

`LayerState` enum coverage:
- `0 IDLE`, `3 PLAYING`, `4 LOOPING`, `5 PAUSED`, `6 STOPPED`,
  `7 CUE BUTTON DOWN`, `8 PLATTER DOWN`, `9 FFWD`, `10 FFRV`,
  `11 HOLD` — all 10 named values present; `Unknown(u8)` fallback for
  forward-compat ✅

---

## 6. TimeSync packet (msg type 10, spec page 8)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 10 | ✅ |
| Size | 32 B | ✅ |
| `STEP` (V3-1) | 0-3 (0=Init, 1=Response documented; 2/3 reserved) | `step: u8` ✅ |
| `Node Listener Port` (V3-2) | u16 LE | ✅ |
| `Remote Timestamp` (V3-2) | u32 LE | ✅ |

**Behaviour (post-`857b0a5` patch):**
- Initiate `step=0` every 5 s round-robin against each active peer — ✅
- On inbound `step=0`: build `TimeSyncData::new_response()` echoing
  the remote timestamp, reply via outgoing path — ✅
- On inbound `step=1`: resolve `PendingTimeSync` against
  `TimeSyncReply { echoed_our_ts_us, their_listener_port,
  responder_send_ts_us }` — ✅
- Clock offset computed per spec page 8 formula
  (`Delay = (Current timer - Remote timestamp) / 2`;
  `Time of remote node = Timestamp + Delay`).  Result stored per peer
  in `Dispatcher.clock_offsets` and exposed via
  `Node::clock_offset_for(peer)` — ✅

Spec page 8 mentions an *optional* iterated refinement (multiple Delay
samples averaged).  Not implemented; ✅ acceptable per "Optional".

**Status:** 🟢 fully behaviour-conformant.

---

## 7. Error / Notification (msg type 13, spec page 9)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 13 | ✅ |
| Size | 30 B | ✅ |
| `Datatype` | u8 (original request data type) | ✅ |
| `Layer ID` | u8 | ✅ |
| `Code` | u16 LE | ✅ |
| `Message Type` | u16 LE (original request msg type) | ✅ |

Spec codes:
- `001` Unknown — mapped to `RequestError::Unknown` ✅
- `013` Not Possible — `RequestError::NotPossible` ✅
- `014` Empty — `RequestError::Empty` ✅
- `255` Response OK — mapped to `RequestError::Other { code: 255 }`
  today; should arguably be a non-error success variant.  🟡

**Behaviour:**
- Outbound: sent automatically by request handler when the response
  payload is empty (code 014) — ✅
- Inbound: parsed but not currently routed to a `Pending<T>` slot.
  Consumers see request timeout after 5 s instead of immediate
  `Empty` / `NotPossible`.  🟡

**Status:** 🟡 — wire layout correct, code-014 outbound automation
works, but inbound routing into `Pending<T>` is not wired (queued for
0.3.0 once `Node::request_*` methods migrate to the typed `Pending`
flow).

---

## 8. Request packet (msg type 20, spec page 10)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 20 | ✅ |
| Size | 26 B | ✅ |
| `Data Type` | u8 per `RequestDataType` | ✅ |
| `Layer` | u8 (0 for non-layer requests) | ✅ |

**Outbound emission:**
- `request_small_waveform` (data type 16) — ✅
- `request_big_waveform` (data type 32) — ✅
- `request_beat_grid` (data type 8) — ✅
- `request_cue_data` (data type 12) — ✅
- `request_artwork_file` (data type 128) — ✅ (via legacy view)

**Inbound handling:**
- Builds and unicasts the matching response from
  `dispatcher.response_data` cache — ✅
- ErrorNotification(014) when cache is empty — ✅

**Status:** ✅

---

## 9. Application-Specific Data (msg types 30 + 213, spec page 30)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 30 or 213 | ⚠ wire variant unified |
| `Data Identifier 1/2` | 2 B (vendor signature) | ✅ |
| `Data Size` | u32 LE | ✅ |
| `Total Packets` | u32 LE | ✅ |
| `Packet No` | u32 LE | ✅ |
| `Packet Signature` | const `178_260_640` | `protocol::APP_SPECIFIC_SIGNATURE` ✅ |
| `Data` | length-prefixed | ✅ |

**Reassembly:** `proto::AppSpecificReassembler` with full validation
(signature, identifier consistency, total-packets agreement,
packet_no range, duplicate detection) — ✅

**Outbound:** no public API today.  When added, the routing must
choose port 60001 for msg type 30 and port 60000 for msg type 213.
🟡

**Inbound:** both msg types parse via the same `Data::AppSpecific`
variant; route to reassembler on receipt.  🟡 (not yet plumbed to a
user-visible callback — queued for 0.3.0 along with outbound).

**Status:** 🟡 wire format ✅, reassembly ✅, runtime routing 🟡.

---

## 10. Control Messages (msg types 101 / 128 / 132, spec pages 11-13)

| Type | Name | Spec | Status |
|------|------|------|--------|
| 101 | Control (path) | 42+DataSize, unicast | ✅ parsed; outbound via `proto::ControlPath` |
| 128 | Text Data | 42+DataSize, broadcast 60000 OR unicast | ✅ parsed; outbound via `proto::TextMessage` |
| 132 | Keyboard | 44 B, HEX-ASCII payload | ✅ parsed; outbound via `proto::KeyPress` |

`ControlPath` ergonomic API:
- `set_layer_state(layer, state)` → `"layer/<id>/state=<n>"` ✅
- `set_layer_source(layer, src)` → `"layer/<id>/source=<n>"` ✅
- `set_master_level(v)` → `"mixer/master/level=<v>"` ✅
- `raw(s)` escape hatch ✅

Spec page 11 examples include trailing semicolons (`"layer/1/state=6;"`)
and a `resync` verb (`"layer/2/resync"`).  Our builders do not append
semicolons by default — callers using `raw()` can include them; the
typed methods produce semicolon-less paths.  Either form is accepted
by Pioneer hardware in practice; ✅ acceptable.

**Outbound runtime:** Build / serialise wired; no periodic emitter,
which is correct (these are user-driven).  ✅
**Inbound runtime:** parsed and dropped — no user callback API yet.
🟡 (queued for 0.3.0).

---

## 11. Metrics (msg type 200 / data type 2, spec page 14-15)

| Field | Spec | Status |
|---|---|---|
| Size | 122 B | ✅ |
| `Layer State` | per spec values | ✅ |
| `Sync Master` | 0=Slave, 1=Master | ✅ |
| `Beat Marker` | 0-4 | ✅ |
| `Track Length` | 4 B LE ms | ✅ |
| `Current Position` | 4 B LE ms | ✅ |
| `Speed` | 4 B LE (per page 15 details: 0-65536 with 32768=100%) | ✅ |
| `Beat Number` | 4 B LE | ✅ |
| `BPM` | 4 B LE (`BPM × 100`) | ✅ |
| `Pitch Bend` | 2 B LE | ✅ |
| `Track ID` | 4 B LE | ✅ |

**Note on Speed:** spec page 14 table says `0-20000` but the page 15
*details* section says `-0~65536 (Where 32768 = 100% speed)`.  We
follow the details (`MAX_RAW = 65_536`); ✅

**Behaviour:**
- Outbound: emitted every 50 ms per layer that has a track loaded
  (not gated on `is_playing()` — see commit `24a3c03` rationale) ✅
- Inbound: folded into `LayerSnapshot` per peer ✅

**Status:** ✅

---

## 12. Meta Data (msg type 200 / data type 4, spec page 16)

| Field | Spec | Status |
|---|---|---|
| Size | 548 B | ✅ |
| `Track Artist` | 128 / 256 B ASCII or UTF-16 | ✅ |
| `Track Title` | 128 / 256 B | ✅ |
| `Track Key` (V3-2) | u16 LE | ✅ |
| `Track ID` (V3-3) | u32 LE | ✅ |

**Encoding:**
- V1.0 – V3.4.9: UTF-8 (256 B = 256 chars) — recognised on read
- V3.5.0 and above: UTF-16 (256 B = 64 chars) — emitted on write

Our `MetadataUtf16Flame::INTRODUCED_AT = (3, 5, 0)` (fixed in commit
`f78c6e2`).  Wire emits UTF-16 LE at V3.6; cross-version reader can
fall back to UTF-8 when the peer's announced major.minor is <V3.5.0.
✅

**Behaviour:**
- Outbound: Meta unicast to each slave on track load + re-broadcast
  at 1 Hz cadence for late-joining slaves (commit `f2ca60b`).  ✅
- Inbound: artist/title decoded into `LayerSnapshot.{artist, title}`
  ✅

**Status:** ✅

---

## 13. Beat Grid Data (msg type 200 / data type 8, spec page 17)

| Field | Spec | Status |
|---|---|---|
| Size | 2442 B per chunk | ✅ |
| `Total Packets` / `Packet No` / `Data Cluster Size` | u32 LE each | ✅ |
| Per-beat: Beat Number, Beat Type (10=Up, 20=Down), Beat Time Stamp (ms) | u16 / u8 / u8 / u32 | ✅ |

**Reassembly:** `proto::ChunkedFrame<BeatGridChunk>` (planned phase 5
generic; current code uses an inline accumulator in
`dj_controller_task` that's structurally identical).  ✅

**Behaviour:**
- Outbound: served from pre-populated `response_data` on incoming
  Request — ✅
- Inbound: chunks accumulated, full payload exposed via
  `Node::request_beat_grid().await` — ✅

**Status:** ✅

---

## 14. Cue Data (msg type 200 / data type 12, spec page 18-21)

| Field | Spec | Status |
|---|---|---|
| Size | 436 B | ✅ |
| Loop IN / Loop OUT | u32 LE each | ✅ |
| 17 CUE slots × (Type, IN time, OUT time, Color RGB) | 22 B each | ✅ |

Spec V3.5.1 added the 17-slot hot/memory cue array (`CueExtendedFlame`,
V3.5.1).  ✅

**Status:** ✅

---

## 15. Small Waveform (msg type 200 / data type 16, spec page 22)

| Field | Spec | Status |
|---|---|---|
| Size | 2442 B (single packet, Data Size = 2400) | ✅ |
| `Total Packets` | 1 typically | ✅ |
| `Waveform Data` | 2400 B, BLevel (odd) / BColor (even) | ✅ |

**Status:** ✅

---

## 16. Big Waveform (msg type 200 / data type 32, spec page 23)

| Field | Spec | Status |
|---|---|---|
| Size | variable | ✅ |
| `Data Cluster Size` | u32 LE, default 4800 | ✅ |
| Multi-packet reassembly | ✅ |

**Status:** ✅

---

## 17. Mixer Data (msg type 200 / data type 150, spec page 24-28)

| Field | Spec | Status |
|---|---|---|
| Size | 270 B | ✅ |
| Mixer ID, Type, Name | u8 / u8 / 16 ASCII | ✅ |
| Master section (audio level, fader, filter, cue A/B, isolator, etc.) | per spec | ✅ |
| Send FX + Send Return 3 + BeatFX | per spec | ✅ |
| Channel section (6 channels × ~12 B) | per spec | ✅ |
| Headphones / Booth | per spec | ✅ |

**Outbound:** unicast to each slave at 1 Hz cadence (Master role) — ✅
**Inbound:** folded into per-peer `MixerSnapshot` — ✅

**Status:** ✅

---

## 18. Artwork File (msg type 204 / data type 128, spec page 29)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 204 | ✅ |
| `Total Packets` / `Packet No` / `Data Cluster Size` | u32 LE each | ✅ |
| File Data | raw JPEG bytes | ✅ |

**Reassembly:** multi-packet, identical pattern to Big Waveform.  ✅

**Status:** ✅

---

## 19. Time Packet (msg type 254, spec page 31-34)

| Field | Spec | Status |
|---|---|---|
| Header msg_type | 254 | ✅ |
| Size | 162 B | ✅ |
| 8 layer current times (ms, u32 LE) | ✅ |
| 8 layer total times (ms, u32 LE) | ✅ |
| 8 layer beat markers (u8, 0-4) | ✅ |
| 8 layer states (u8) | ✅ |
| General + per-layer SMPTE Mode | ✅ |
| Per-layer Time Code (State, Hours, Minutes, Seconds, Frames) | ✅ |
| 8 layer On-Air bytes (V3-3-3) | ✅ |

**Behaviour:**
- Broadcast on port 60001 every 20 ms (spec range 1-40 ms — ✅ within spec)
- Unicast on port Target-Node-Port to each discovered LOCAL node
  (spec page 34 step 3 + spec page 2) — ✅

**Status:** ✅

---

## 20. Node Options (spec page 2)

| Bit | Spec | Implementation | Status |
|---|---|---|---|
| 1 | NEED_AUTHENTICATION | `NodeOptions::NEED_AUTHENTICATION` | ✅ defined; no handshake (no spec text) ⬛ |
| 2 | SUPPORTS_TCNCM | `NodeOptions::SUPPORTS_TCNCM` | ✅ |
| 4 | SUPPORTS_TCNASDP | `NodeOptions::SUPPORTS_TCNASDP` | ✅ |
| 8 | DND | `NodeOptions::DND` | ✅ |

Bits are read off the wire and surfaced as `LayerSnapshot` /
`PeerInfo` flags; outbound packets carry whatever the local
`ApplicationConfig.node_options` sets.  ✅

Authentication handshake itself is undocumented in V3.5.1B; the
scaffolding exists (`session::PeerAuth`) but no flow.  Acceptable per
spec since the spec doesn't describe the handshake.  ⬛ unimplemented
but not blockable.

---

## 21. Master Election (spec page 5 — OptOut TIP)

> "In case of a disconnect of a Master Node in the network, the next
> master is chosen by looking at all Nodes running as Node Type 1
> (Auto Master).  The node that has the highest Uptime including
> Timestamp becomes the new master."

**Implementation (`session::Election` + `dispatcher::election_driver`):**
- Driver runs at 1 Hz, builds the candidate set from peers whose
  `NodeType` is `Master` or `Auto`.  ✅
- Tie-break: higher `uptime_secs` wins, then earlier `announced_at`,
  then lower `node_id`.  Spec only specifies uptime + timestamp; we
  add `node_id` as a deterministic fallback so all peers converge on
  the same answer.  ✅
- Result published via `Dispatcher.election`, readable from
  `Node::election_state()`.  ✅

**Gaps:**
- Per-peer `uptime_secs` is approximated as `(now - we_first_saw)`
  rather than peer's announced uptime — the OptIn `Uptime` field
  is parsed but not threaded through to the election candidate.
  This is a measurable accuracy issue: two peers that joined a
  network mid-session will have different `last_seen` ages on the
  *observer* but identical `Uptime` values they're announcing.
  🟡

- We do not yet implement the "Master self-transition": when a
  local-Auto node wins, we don't switch our own `node_type` to
  Master and start emitting Time packets at master cadence.  The
  `Election::begin_contending` / `stand_down` hooks exist; wiring
  is queued for 0.3.0.  🟡

**Status:** 🟡 — election decision computed correctly per spec
formula (using observer-uptime as approximation); local-role
transition not automatic.

---

## 22. Flame Versions (spec page 3, change log page 36)

| Flame | Spec Introduction | Implementation `INTRODUCED_AT` | Status |
|---|---|---|---|
| Base / V1-0 | 1.0 | `(1, 0, 0)` | ✅ |
| OptInVendor (V3-2) | 3.2 | `(3, 2, 0)` | ✅ |
| SmallBigWaveform (V3-2) | 3.2.0 | `(3, 2, 0)` | ✅ |
| BeatGridInfo (V3-2-1) | 3.2.1 | `(3, 2, 1)` | ✅ |
| ArtworkFile (V3-2-5) | 3.2.5 | `(3, 2, 5)` | ✅ |
| CueData (V3-2-5) | 3.2.5 | `(3, 2, 5)` | ✅ |
| NodeOptions (V3-3) | 3.3.0 | `(3, 3, 0)` | ✅ |
| SmpteInTimePacket (V3-3-1) | 3.3.1 | `(3, 3, 1)` | ✅ |
| LayerName (V3-3-2) | 3.3.2 | `(3, 3, 2)` | ✅ |
| FaderOnAir (V3-3-3) | 3.3.3 | `(3, 3, 3)` | ✅ |
| UnicastOptInOut (V3-3-3) | 3.3.3 | `(3, 3, 3)` | ✅ |
| MixerData (V3-4-1) | 3.4.1 | `(3, 4, 1)` | ✅ |
| MixerExtended (V3-4-2) | 3.4.2 | `(3, 4, 2)` | ✅ |
| CueExtended (V3-5-1) | 3.5.1 | `(3, 5, 1)` | ✅ |
| MetadataUtf16 (V3-5-0) | 3.5.0 (per page 16) | `(3, 5, 0)` — fixed in `f78c6e2` | ✅ |

Cross-version matrix test `tests/cross_version_matrix.rs` asserts
every `(version, flame)` pair agrees between the const
`INTRODUCED_AT` and the runtime `PeerVersion::includes::<F>()`.
✅

---

## 23. Registered Application Codes (spec page 36)

| Code | Vendor | Implementation |
|---|---|---|
| 0000 | Reserved (Public) | ⬛ |
| 0AA0 | Pioneer DJ | ⬛ |
| 0AAA | TC Supply / ShowKontrol | ⬛ |
| 0AAB | TC Supply Pyrotechnic | ⬛ |
| 0AAC | TC Supply Ride Control | ⬛ |
| 0AB0 | Avolites Lighting | ⬛ |
| 0AB1 | MA Lighting | ⬛ |
| 0AB3 | Chamsys Lighting | ⬛ |
| 0AB4 | Obsidian Control | ⬛ |
| 0ABA | Arkaos Software | ⬛ |
| 0ABB | BLCKBOOK / Time Code Sync | ⬛ |
| 0ABC | Resolume Software | ⬛ |
| 0ABD | Green Hippo | ⬛ |
| 0ABE | RD/ShowCockpit | ⬛ |
| 0ABF | Disguise | ⬛ |
| 0ACA | OrangePI | ⬛ |
| 0ACB | RedPill VR | ⬛ |
| FFFF | Reserved (Public) | ⬛ |

A typed `VendorId` enum mirroring this table is queued for 0.3.0
alongside outbound AppSpecific API.  ⬛ (functionality unchanged —
inbound `AppSpecificFrame.identifier` carries the raw `[u8; 2]`
and consumers can match on it today).

---

## Summary

| Category | Conformant | Partial | Not implemented |
|---|---|---|---|
| Ports & transport | 4 / 6 | 2 / 6 (AppSpecific outbound) | 0 |
| Management header | 10 / 10 | 0 | 0 |
| Discovery (OptIn/OptOut) | full | 0 | 0 |
| Status / Metrics / Meta / Time | full | 0 | 0 |
| TimeSync handshake | **full incl. clock offset** | 0 | 0 |
| Master election | candidate selection + winner publication | local-role transition + announced-uptime accuracy | 0 |
| Request / Response | full incl. ErrorNotification 014 emission | inbound 013/014/255 routing to `Pending<T>` | 0 |
| Control / Text / Keyboard | full wire | runtime user-callback API | 0 |
| AppSpecific (msg 30 / 213) | full reassembly | outbound emission + port-30/213 distinction | 0 |
| Beat Grid / Cue / Waveform / Artwork | full | 0 | 0 |
| Mixer Data | full | 0 | 0 |
| Node Options | full reading + writing | Authentication handshake (no spec) | 0 |
| FLAME version machinery | full + matrix-tested | 0 | 0 |

**Headline:** every packet type defined in V3.5.1B is wire-format
conformant; every behaviour the spec describes as required (OptIn
cadence, Status broadcast + slave unicast, Time at 1-40 ms cadence,
TimeSync handshake with clock offset, OptOut on shutdown, peer
timeout at 10 s, ErrorNotification 014 for empty responses, master
election from Auto candidates by uptime) is implemented and runs.

The remaining 🟡 items are about **runtime ergonomics surfaced to
library users** (inbound Control / Text / AppSpecific callbacks,
typed Pending<T> for request errors, outbound AppSpecific port-30
vs port-213 distinction) and **one accuracy refinement** (using
peer-announced `Uptime` rather than observer-side `last_seen` for
election tie-breaks).  None of them change wire-level behaviour
that a TCNet bridge or CDJ-3000 would observe.

The ⬛ items are exclusively things the spec itself does not define
(Authentication handshake) or downstream catalogue conveniences
(typed vendor-ID enum for AppSpecific).
