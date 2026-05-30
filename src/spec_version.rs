//! TCNet protocol-version machinery (ARCHITECTURE.md §12).
//!
//! Every TCNet packet field is tagged in the spec with the *FLAME*
//! revision in which it was added (`V3-3`, `V3-3-2`, `V3-5`, …).  A peer
//! claiming `protocol_version = 3.6` is not obligated to send fields
//! that were introduced after its firmware was cut; a peer running a
//! future revision may emit trailing bytes we don't yet understand.
//!
//! Two parallel type families capture this in the type system:
//!
//! * [`SpecVersion`] — a marker for *which revision the local node
//!   speaks*. The local node always emits at its declared `SpecVersion`
//!   and no later. Implementations: [`V1_0`], …, [`V3_6`].
//! * [`Flame`] — a marker for *one introduction event* (e.g. "Layer
//!   names were added"). Implementations: [`LayerNameFlame`],
//!   [`NodeOptionsFlame`], [`MixerDataFlame`], …
//!
//! [`IncludesFlame<F>`] is the relation: `V: IncludesFlame<F>` means
//! "version `V` knows about FLAME `F`". A builder method that depends
//! on a late field is gated by an `IncludesFlame` bound, so a
//! `Node<Slave, V3_3_2>` does not see `.with_mixer_*` methods
//! introduced at `V3_4_1`.
//!
//! Versions reconstructed from the change log of TCNet V3.5.1B
//! (`docs/spec/TCNet-V3-5-1B.pdf`, pages 35-36).

/// Sealed-trait pattern: only types in this crate may implement
/// `SpecVersion` / `Flame` / `IncludesFlame`.
mod sealed {
    pub trait SpecVersionSealed {}
    pub trait FlameSealed {}
}

// ---------------------------------------------------------------------------
// SpecVersion marker types — one per spec revision that introduces a
// FLAME we care about.
// ---------------------------------------------------------------------------

/// A spec revision the local node may speak.
///
/// `MAJOR.MINOR.PATCH` is the canonical `X.Y.Z` form (e.g. `3.5.1`); the
/// `ManagementHeader.protocol_version_*` bytes always emit `MAJOR` and
/// `MINOR` (the spec wire layout treats those as static fields).
pub trait SpecVersion: sealed::SpecVersionSealed + Copy + Default + 'static {
    const MAJOR: u8;
    const MINOR: u8;
    const PATCH: u8;
    /// Human-readable revision string e.g. `"V3.5.1"`.
    const REVISION: &'static str;
}

macro_rules! decl_spec_version {
    ($name:ident, $maj:literal, $min:literal, $pat:literal, $rev:literal) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
        pub struct $name;
        impl sealed::SpecVersionSealed for $name {}
        impl SpecVersion for $name {
            const MAJOR: u8 = $maj;
            const MINOR: u8 = $min;
            const PATCH: u8 = $pat;
            const REVISION: &'static str = $rev;
        }
    };
}

decl_spec_version!(V1_0, 1, 0, 0, "V1.0");
decl_spec_version!(V2_0, 2, 0, 0, "V2.0");
decl_spec_version!(V2_1, 2, 1, 0, "V2.1");
decl_spec_version!(V3_0, 3, 0, 0, "V3.0");
decl_spec_version!(V3_1, 3, 1, 0, "V3.1");
decl_spec_version!(V3_2, 3, 2, 0, "V3.2");
decl_spec_version!(V3_3, 3, 3, 0, "V3.3");
decl_spec_version!(V3_3_1, 3, 3, 1, "V3.3.1");
decl_spec_version!(V3_3_2, 3, 3, 2, "V3.3.2");
decl_spec_version!(V3_3_3, 3, 3, 3, "V3.3.3");
decl_spec_version!(V3_4_1, 3, 4, 1, "V3.4.1");
decl_spec_version!(V3_4_2, 3, 4, 2, "V3.4.2");
decl_spec_version!(V3_5, 3, 5, 0, "V3.5");
decl_spec_version!(V3_5_1, 3, 5, 1, "V3.5.1");
// V3.5.1B corrected UTF-16 in metadata — we treat it as the same
// wire-level version as V3.5.1, but it's the firmware-version string
// most CDJ-3000s now ship with. The wire-level "protocol_version_minor"
// byte we emit is `6` — that field hasn't changed since V1.0 per the
// spec table (page 4).
decl_spec_version!(V3_6, 3, 6, 0, "V3.6");

// ---------------------------------------------------------------------------
// FLAMEs — one marker per "this field landed at version X" event.
//
// Naming convention: `<Feature>Flame`. Each Flame's docstring cites the
// page / change-log entry it came from in TCNet-V3-5-1B.pdf.
// ---------------------------------------------------------------------------

/// A single introduction-event in the TCNet spec.
pub trait Flame: sealed::FlameSealed + Copy + Default + 'static {
    /// First spec version that knew about this field.
    const INTRODUCED_AT: (u8, u8, u8);
    /// Human-readable label e.g. `"Layer names"`.
    const LABEL: &'static str;
}

macro_rules! decl_flame {
    ($name:ident, ($maj:literal, $min:literal, $pat:literal), $label:literal) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
        pub struct $name;
        impl sealed::FlameSealed for $name {}
        impl Flame for $name {
            const INTRODUCED_AT: (u8, u8, u8) = ($maj, $min, $pat);
            const LABEL: &'static str = $label;
        }
    };
}

// Listed roughly in chronological order. Sources are change-log entries
// on spec pages 35-36; per-field tags in the body tables on pages 4-34.
decl_flame!(BaseFlame, (1, 0, 0), "TCNet base protocol (V1.0)");
decl_flame!(OptInVendorFlame, (3, 2, 0), "Vendor / application / version fields in OptIn");
decl_flame!(SmallBigWaveformFlame, (3, 2, 0), "Small / Big waveform packet replacement");
decl_flame!(BeatGridInfoFlame, (3, 2, 1), "Beat Grid Info packet");
decl_flame!(ArtworkFileFlame, (3, 2, 5), "Artwork File packet");
decl_flame!(CueDataFlame, (3, 2, 5), "Cue Data packet");
decl_flame!(NodeOptionsFlame, (3, 3, 0), "NodeOptions header byte");
decl_flame!(SmpteInTimePacketFlame, (3, 3, 1), "SMPTE values back in Time packets");
decl_flame!(LayerNameFlame, (3, 3, 2), "Layer-Name fields in Status packet");
decl_flame!(FaderOnAirFlame, (3, 3, 3), "Per-layer fader-position On-Air bytes in Time packets");
decl_flame!(UnicastOptInOutFlame, (3, 3, 3), "Unicast OptIn / OptOut / Time emissions");
decl_flame!(BcolorExplanationFlame, (3, 3, 3), "Bcolor byte-pair explanation for waveforms");
decl_flame!(MixerDataFlame, (3, 4, 1), "Mixer Data packet (msg 200 / data type 150)");
decl_flame!(MixerExtendedFlame, (3, 4, 2), "Extended data fields on Mixer Data");
decl_flame!(CueExtendedFlame, (3, 5, 1), "Cue Data for Hot / Memory cues");
decl_flame!(MetadataUtf16Flame, (3, 5, 1), "UTF-16 encoding in MetaData (was UTF-8)");

// ---------------------------------------------------------------------------
// IncludesFlame — the version-knows-about-FLAME relation.
//
// `V: IncludesFlame<F>` is implemented exactly when `V` was cut at or
// after `F::INTRODUCED_AT`. Manual impls per (version, flame) pair —
// tedious but compile-time exact.
//
// Convention: each version implements `IncludesFlame` for every Flame
// at or before its introduction date. Since this is monotone, we get a
// staircase: V3_6 includes everything; V3_5 excludes only V3_6's
// additions; etc.
// ---------------------------------------------------------------------------

/// `V: IncludesFlame<F>` ⇔ `V` was cut at or after `F::INTRODUCED_AT`.
pub trait IncludesFlame<F: Flame>: SpecVersion {}

/// Macro: "version `$v` includes flames `$($f),+`".
///
/// Use this when adding a new version: list every flame whose
/// `INTRODUCED_AT` is `<= (v.major, v.minor, v.patch)`.
macro_rules! includes {
    ($v:ty : $($f:ty),+ $(,)?) => {
        $( impl IncludesFlame<$f> for $v {} )+
    };
}

// V1_0 / V2_0 / V2_1 / V3_0 / V3_1 only know about BaseFlame —
// everything else was added in V3_2+. Their primary use today is
// representing peers that come up on the wire announcing those old
// minor versions; we mostly never emit at those versions ourselves.
includes!(V1_0: BaseFlame);
includes!(V2_0: BaseFlame);
includes!(V2_1: BaseFlame);
includes!(V3_0: BaseFlame);
includes!(V3_1: BaseFlame);

// V3.2 added the vendor fields in OptIn, replaced small/big waveform,
// added beat grid info, then artwork + cue.
includes!(V3_2:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
);

// V3.3 — NodeOptions header byte.
includes!(V3_3:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
);

// V3.3.1 — SMPTE re-added.
includes!(V3_3_1:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
    SmpteInTimePacketFlame,
);

// V3.3.2 — Layer name in Status.
includes!(V3_3_2:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
    SmpteInTimePacketFlame,
    LayerNameFlame,
);

// V3.3.3 — fader on-air, unicast OptIn, Bcolor explanation.
includes!(V3_3_3:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
    SmpteInTimePacketFlame,
    LayerNameFlame,
    FaderOnAirFlame,
    UnicastOptInOutFlame,
    BcolorExplanationFlame,
);

// V3.4.1 — Mixer Data packet.
includes!(V3_4_1:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
    SmpteInTimePacketFlame,
    LayerNameFlame,
    FaderOnAirFlame,
    UnicastOptInOutFlame,
    BcolorExplanationFlame,
    MixerDataFlame,
);

// V3.4.2 — extended Mixer fields.
includes!(V3_4_2:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
    SmpteInTimePacketFlame,
    LayerNameFlame,
    FaderOnAirFlame,
    UnicastOptInOutFlame,
    BcolorExplanationFlame,
    MixerDataFlame,
    MixerExtendedFlame,
);

// V3.5 — superset of V3.4.2 with no new field-level additions in this
// revision (clean-ups only).
includes!(V3_5:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
    SmpteInTimePacketFlame,
    LayerNameFlame,
    FaderOnAirFlame,
    UnicastOptInOutFlame,
    BcolorExplanationFlame,
    MixerDataFlame,
    MixerExtendedFlame,
);

// V3.5.1 — extended Cue (hot/memory), UTF-16 metadata.
includes!(V3_5_1:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
    SmpteInTimePacketFlame,
    LayerNameFlame,
    FaderOnAirFlame,
    UnicastOptInOutFlame,
    BcolorExplanationFlame,
    MixerDataFlame,
    MixerExtendedFlame,
    CueExtendedFlame,
    MetadataUtf16Flame,
);

// V3.6 — superset of V3.5.1 — what we emit today.
includes!(V3_6:
    BaseFlame,
    OptInVendorFlame,
    SmallBigWaveformFlame,
    BeatGridInfoFlame,
    ArtworkFileFlame,
    CueDataFlame,
    NodeOptionsFlame,
    SmpteInTimePacketFlame,
    LayerNameFlame,
    FaderOnAirFlame,
    UnicastOptInOutFlame,
    BcolorExplanationFlame,
    MixerDataFlame,
    MixerExtendedFlame,
    CueExtendedFlame,
    MetadataUtf16Flame,
);

// ---------------------------------------------------------------------------
// Runtime peer-version representation
//
// The compile-time `SpecVersion` markers describe what *we* emit. Peers
// announce a `(major, minor)` pair at runtime via `ManagementHeader`.
// `PeerVersion` is the runtime carrier; helper predicates ask whether
// it includes a given FLAME.
// ---------------------------------------------------------------------------

/// A peer's declared (major, minor) protocol version, read from its
/// [`ManagementHeader`](crate::protocol::ManagementHeader).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerVersion {
    pub major: u8,
    pub minor: u8,
}

impl PeerVersion {
    pub const fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }

    /// Return `true` if a peer at this version is known to include
    /// FLAME `F`. Compares `(major, minor, 0)` against
    /// `F::INTRODUCED_AT`.
    pub fn includes<F: Flame>(self) -> bool {
        let (fm, fmi, _fp) = F::INTRODUCED_AT;
        (self.major, self.minor) >= (fm, fmi)
    }
}

impl<V: SpecVersion> From<V> for PeerVersion {
    fn from(_v: V) -> Self {
        Self::new(V::MAJOR, V::MINOR)
    }
}

impl PeerVersion {
    /// Read a peer's protocol version out of the [`ManagementHeader`]
    /// `protocol_version_major` / `protocol_version_minor` bytes.
    ///
    /// This is the runtime equivalent of the compile-time
    /// [`SpecVersion`] markers — once a packet has been parsed we don't
    /// know its peer's `SpecVersion` as a type, only as a `(major,
    /// minor)` pair on the wire. `PeerVersion::from_header` is the
    /// bridge between the two worlds.
    ///
    /// [`ManagementHeader`]: crate::protocol::ManagementHeader
    pub fn from_header(h: &crate::protocol::ManagementHeader) -> Self {
        Self::new(h.protocol_version_major, h.protocol_version_minor)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_version_constants_round_trip() {
        assert_eq!(V3_6::MAJOR, 3);
        assert_eq!(V3_6::MINOR, 6);
        assert_eq!(V3_3_2::REVISION, "V3.3.2");
    }

    #[test]
    fn includes_flame_compile_time_gating() {
        // These are pure type-system assertions; the runtime check is
        // a tautology, but the trait-bound check at compile-time is the
        // actual test.
        fn _gate<V: IncludesFlame<MixerDataFlame>>() {}
        _gate::<V3_4_1>();
        _gate::<V3_5>();
        _gate::<V3_6>();
        // A `_gate::<V3_3_2>()` would not compile — V3_3_2 does not
        // implement IncludesFlame<MixerDataFlame>.
    }

    #[test]
    fn peer_version_includes_matches_flame_intro_date() {
        assert!(PeerVersion::new(3, 6).includes::<MixerDataFlame>());
        assert!(PeerVersion::new(3, 4).includes::<MixerDataFlame>());
        assert!(!PeerVersion::new(3, 3).includes::<MixerDataFlame>());
        assert!(!PeerVersion::new(3, 2).includes::<MixerDataFlame>());

        assert!(PeerVersion::new(3, 6).includes::<LayerNameFlame>());
        assert!(PeerVersion::new(3, 3).includes::<LayerNameFlame>());
        // V3.3.2 introduced LayerName; PeerVersion is (major,minor)
        // only so this is the coarsest answer we can give. Field-level
        // detail comes from the read side (Option::None for missing).
        assert!(PeerVersion::new(3, 3).includes::<LayerNameFlame>());
        assert!(!PeerVersion::new(3, 2).includes::<LayerNameFlame>());
        assert!(!PeerVersion::new(3, 0).includes::<LayerNameFlame>());
    }

    #[test]
    fn peer_version_from_spec_version_type() {
        let pv: PeerVersion = V3_6.into();
        assert_eq!(pv, PeerVersion::new(3, 6));
        let pv: PeerVersion = V3_3_2.into();
        assert_eq!(pv, PeerVersion::new(3, 3));
    }

    #[test]
    fn peer_version_from_header_reads_wire_bytes() {
        use crate::protocol::ManagementHeader;
        let h = ManagementHeader {
            node_id: 0,
            protocol_version_major: 3,
            protocol_version_minor: 6,
            _header: crate::into_ascii!("TCN"),
            message_type: 0,
            node_name: crate::into_ascii!("Test____"),
            seq: 0,
            node_type: crate::protocol::NodeType::Slave,
            node_options: crate::protocol::NodeOptions::empty(),
            timestamp: 0,
        };
        assert_eq!(PeerVersion::from_header(&h), PeerVersion::new(3, 6));

        let h2 = ManagementHeader {
            protocol_version_minor: 3,
            ..h
        };
        assert_eq!(PeerVersion::from_header(&h2), PeerVersion::new(3, 3));
    }

    // ----------------------------------------------------------------
    // Builder-gating demo — proves the IncludesFlame relation actually
    // gates methods at compile time. Phase 5 wires real builders to
    // this pattern; this module exists so the regression has a
    // dedicated home now.
    // ----------------------------------------------------------------

    mod gating_demo {
        use super::super::*;
        use std::marker::PhantomData;

        #[derive(Default)]
        struct DemoBuilder<V: SpecVersion> {
            layer: u8,
            layer_name: Option<[u8; 16]>,
            mixer_fader_a: Option<u8>,
            _v: PhantomData<V>,
        }

        impl<V: SpecVersion> DemoBuilder<V> {
            fn new() -> Self {
                Self::default()
            }
            fn with_layer(mut self, layer: u8) -> Self {
                self.layer = layer;
                self
            }
        }

        // Only versions that include LayerNameFlame may set a name.
        impl<V: SpecVersion + IncludesFlame<LayerNameFlame>> DemoBuilder<V> {
            fn with_layer_name(mut self, name: [u8; 16]) -> Self {
                self.layer_name = Some(name);
                self
            }
        }

        // Only versions that include MixerDataFlame may set mixer fields.
        impl<V: SpecVersion + IncludesFlame<MixerDataFlame>> DemoBuilder<V> {
            fn with_mixer_fader_a(mut self, level: u8) -> Self {
                self.mixer_fader_a = Some(level);
                self
            }
        }

        #[test]
        fn gating_demo_v3_6_has_all_methods() {
            let b = DemoBuilder::<V3_6>::new()
                .with_layer(1)
                .with_layer_name([b'A'; 16])
                .with_mixer_fader_a(127);
            assert_eq!(b.layer, 1);
            assert!(b.layer_name.is_some());
            assert!(b.mixer_fader_a.is_some());
        }

        #[test]
        fn gating_demo_v3_3_2_has_layer_name_but_not_mixer() {
            let b = DemoBuilder::<V3_3_2>::new()
                .with_layer(2)
                .with_layer_name([b'B'; 16]);
            assert_eq!(b.layer, 2);
            assert!(b.layer_name.is_some());
            // The following line would NOT compile (commented out by
            // design; uncommenting must produce E0599 "method not found
            // in `DemoBuilder<V3_3_2>`"):
            //
            //   let _ = b.with_mixer_fader_a(127);
        }

        #[test]
        fn gating_demo_v3_3_has_neither_extension() {
            let b = DemoBuilder::<V3_3>::new().with_layer(3);
            assert_eq!(b.layer, 3);
            // Neither of these compiles for V3_3 — V3.3 predates both
            // LayerName (V3.3.2) and MixerData (V3.4.1):
            //
            //   let _ = b.with_layer_name([0; 16]);
            //   let _ = b.with_mixer_fader_a(0);
        }
    }
}
