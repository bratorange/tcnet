//! Cross-version FLAME matrix test (ARCHITECTURE.md §12 + plan phase 9).
//!
//! For every `(local, remote)` SpecVersion pair in our marker set,
//! check the FLAME inclusion machinery agrees with the runtime
//! `PeerVersion::includes` predicate.  This proves the
//! compile-time `IncludesFlame<F>` trait impls and the runtime
//! tag derived from `INTRODUCED_AT` constants don't drift apart as
//! versions get added.
//!
//! Golden-vector pcap replay (the other half of phase 9) is omitted
//! — capturing a 30-second bridge session needs the live bridge
//! running, which isn't in CI scope; the manual smoke check lives
//! in the plan's bridge-integration workflow.

use tcnet::spec_version::*;

/// Every SpecVersion we ship.
fn all_versions() -> Vec<(&'static str, (u8, u8, u8))> {
    vec![
        (V1_0::REVISION, (V1_0::MAJOR, V1_0::MINOR, V1_0::PATCH)),
        (V2_0::REVISION, (V2_0::MAJOR, V2_0::MINOR, V2_0::PATCH)),
        (V2_1::REVISION, (V2_1::MAJOR, V2_1::MINOR, V2_1::PATCH)),
        (V3_0::REVISION, (V3_0::MAJOR, V3_0::MINOR, V3_0::PATCH)),
        (V3_1::REVISION, (V3_1::MAJOR, V3_1::MINOR, V3_1::PATCH)),
        (V3_2::REVISION, (V3_2::MAJOR, V3_2::MINOR, V3_2::PATCH)),
        (V3_3::REVISION, (V3_3::MAJOR, V3_3::MINOR, V3_3::PATCH)),
        (V3_3_1::REVISION, (V3_3_1::MAJOR, V3_3_1::MINOR, V3_3_1::PATCH)),
        (V3_3_2::REVISION, (V3_3_2::MAJOR, V3_3_2::MINOR, V3_3_2::PATCH)),
        (V3_3_3::REVISION, (V3_3_3::MAJOR, V3_3_3::MINOR, V3_3_3::PATCH)),
        (V3_4_1::REVISION, (V3_4_1::MAJOR, V3_4_1::MINOR, V3_4_1::PATCH)),
        (V3_4_2::REVISION, (V3_4_2::MAJOR, V3_4_2::MINOR, V3_4_2::PATCH)),
        (V3_5::REVISION, (V3_5::MAJOR, V3_5::MINOR, V3_5::PATCH)),
        (V3_5_1::REVISION, (V3_5_1::MAJOR, V3_5_1::MINOR, V3_5_1::PATCH)),
        (V3_6::REVISION, (V3_6::MAJOR, V3_6::MINOR, V3_6::PATCH)),
    ]
}

/// Every Flame we ship, paired with a "test function" that checks
/// `PeerVersion::includes::<F>()`.
fn flame_introductions() -> Vec<(&'static str, (u8, u8, u8), fn(PeerVersion) -> bool)> {
    vec![
        (
            BaseFlame::LABEL,
            BaseFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<BaseFlame>(),
        ),
        (
            OptInVendorFlame::LABEL,
            OptInVendorFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<OptInVendorFlame>(),
        ),
        (
            SmallBigWaveformFlame::LABEL,
            SmallBigWaveformFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<SmallBigWaveformFlame>(),
        ),
        (
            BeatGridInfoFlame::LABEL,
            BeatGridInfoFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<BeatGridInfoFlame>(),
        ),
        (
            ArtworkFileFlame::LABEL,
            ArtworkFileFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<ArtworkFileFlame>(),
        ),
        (
            CueDataFlame::LABEL,
            CueDataFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<CueDataFlame>(),
        ),
        (
            NodeOptionsFlame::LABEL,
            NodeOptionsFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<NodeOptionsFlame>(),
        ),
        (
            SmpteInTimePacketFlame::LABEL,
            SmpteInTimePacketFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<SmpteInTimePacketFlame>(),
        ),
        (
            LayerNameFlame::LABEL,
            LayerNameFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<LayerNameFlame>(),
        ),
        (
            FaderOnAirFlame::LABEL,
            FaderOnAirFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<FaderOnAirFlame>(),
        ),
        (
            UnicastOptInOutFlame::LABEL,
            UnicastOptInOutFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<UnicastOptInOutFlame>(),
        ),
        (
            BcolorExplanationFlame::LABEL,
            BcolorExplanationFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<BcolorExplanationFlame>(),
        ),
        (
            MixerDataFlame::LABEL,
            MixerDataFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<MixerDataFlame>(),
        ),
        (
            MixerExtendedFlame::LABEL,
            MixerExtendedFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<MixerExtendedFlame>(),
        ),
        (
            CueExtendedFlame::LABEL,
            CueExtendedFlame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<CueExtendedFlame>(),
        ),
        (
            MetadataUtf16Flame::LABEL,
            MetadataUtf16Flame::INTRODUCED_AT,
            |pv: PeerVersion| pv.includes::<MetadataUtf16Flame>(),
        ),
    ]
}

#[test]
fn every_version_includes_every_earlier_flame() {
    for (v_label, (vmaj, vmin, _vpatch)) in all_versions() {
        let pv = PeerVersion::new(vmaj, vmin);
        for (f_label, (fmaj, fmin, _fpatch), pred) in flame_introductions() {
            let expected = (vmaj, vmin) >= (fmaj, fmin);
            let got = pred(pv);
            assert_eq!(
                got, expected,
                "version {v_label} ({vmaj}.{vmin}) includes {f_label} ({fmaj}.{fmin})? expected={expected}, got={got}",
            );
        }
    }
}

#[test]
fn from_header_round_trips_for_every_version() {
    use tcnet::into_ascii;
    use tcnet::protocol::{ManagementHeader, NodeOptions, NodeType};

    for (v_label, (vmaj, vmin, _vpatch)) in all_versions() {
        let h = ManagementHeader {
            node_id: 0,
            protocol_version_major: vmaj,
            protocol_version_minor: vmin,
            _header: into_ascii!("TCN"),
            message_type: 0,
            node_name: into_ascii!("xv_test_"),
            seq: 0,
            node_type: NodeType::Slave,
            node_options: NodeOptions::empty(),
            timestamp: 0,
        };
        let pv = PeerVersion::from_header(&h);
        assert_eq!(
            pv,
            PeerVersion::new(vmaj, vmin),
            "PeerVersion::from_header mismatch for {v_label}"
        );
    }
}

#[test]
fn local_v3_6_includes_every_shipped_flame() {
    // V3_6 is what we emit today — it must know about every Flame.
    let pv = PeerVersion::from(V3_6);
    for (label, (fmaj, fmin, _), pred) in flame_introductions() {
        assert!(
            pred(pv),
            "V3_6 should include {label} (introduced {fmaj}.{fmin})"
        );
    }
}

#[test]
fn local_v1_0_only_includes_base_flame() {
    let pv = PeerVersion::from(V1_0);
    assert!(pv.includes::<BaseFlame>());
    // Anything introduced ≥ V2.0 is excluded.
    assert!(!pv.includes::<MixerDataFlame>());
    assert!(!pv.includes::<MetadataUtf16Flame>());
    assert!(!pv.includes::<NodeOptionsFlame>());
}
