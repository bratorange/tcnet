//! Integration tests: a fake CDJ node sends real TCNet UDP packets to a
//! TCNetClient running on 127.0.0.1, then asserts the resulting
//! DjControllerView reflects the expected state.
//!
//! Run with:
//!   cargo test -- --test-threads=1 --nocapture
//! (single-threaded because the client binds fixed ports 60000-60002 + 65023)

use crate::into_ascii;
use crate::node::tcnet_packet::management_header;
use crate::protocol::{
    AutoMasterMode, LayerId, LayerState, LayerStatus, LayerTimecode, MetricsData, MixerChannel,
    MixerData, NodeOptions, NodeType, OptInData, RequestData, RequestDataType, StatusData,
    TimePacketData,
};
use crate::{ApplicationConfig, TCNetClient};
use deku::DekuContainerWrite;
use log::trace;
use serial_test::serial;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::thread::sleep;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Test constants – a simple two-CDJ, one-mixer scenario
// ---------------------------------------------------------------------------

const TRACK_1_ID: u32 = 1001;
const TRACK_2_ID: u32 = 1002;
const TRACK_1_BPM: u32 = 13_400; // 134.00 BPM × 100
const TRACK_1_POS: u32 = 45_000; // 45 s into track
const TRACK_1_LEN: u32 = 360_000; // 6 min total

const MASTER_FADER: u8 = 200;
const CH1_FADER: u8 = 180;
const CH2_FADER: u8 = 0; // channel 2 faded out
const CROSSFADER: u8 = 128; // center

// ---------------------------------------------------------------------------
// Fake CDJ helpers
// ---------------------------------------------------------------------------

fn fake_config() -> ApplicationConfig {
    ApplicationConfig {
        node_id: 42,
        node_type: NodeType::Slave,
        vendor_name: into_ascii!("Pioneer_________"),
        application_name: into_ascii!("rekordbox_______"),
        application_major_version: 6,
        application_minor_version: 0,
        application_bug_version: 0,
        node_name: into_ascii!("CDJ-3000"),
        node_options: NodeOptions::empty(),
        address: SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 60_023),
    }
}

fn send_packet(sock: &UdpSocket, dest: SocketAddr, header_bytes: Vec<u8>, data_bytes: Vec<u8>) {
    let payload = [header_bytes, data_bytes].concat();
    sock.send_to(&payload, dest).expect("send_to failed");
}

fn opt_in_bytes(config: &ApplicationConfig, seq: u8) -> (Vec<u8>, Vec<u8>) {
    let header = management_header(config, 2, seq);
    let data = OptInData {
        node_count: 1,
        node_listener_port: config.address.port(),
        uptime: 0,
        _reserved0: Default::default(),
        vendor_name: config.vendor_name,
        application: config.application_name,
        application_major_version: config.application_major_version,
        application_minor_version: config.application_minor_version,
        application_bug_version: config.application_bug_version,
        _reserved1: Default::default(),
    };
    (header.to_bytes().unwrap(), data.to_bytes().unwrap())
}

fn status_bytes(config: &ApplicationConfig, seq: u8) -> (Vec<u8>, Vec<u8>) {
    let header = management_header(config, 5, seq);

    let mut layer_1_name = [0u8; 16];
    let src = b"Track One";
    layer_1_name[..src.len()].copy_from_slice(src);

    let mut layer_2_name = [0u8; 16];
    let src = b"Track Two";
    layer_2_name[..src.len()].copy_from_slice(src);

    let data = StatusData {
        node_count: 1,
        node_listener_port: config.address.port(),
        _reserved0: [0u8; 6],
        layer_1_source: 1,
        layer_2_source: 2,
        layer_3_source: 0,
        layer_4_source: 0,
        layer_a_source: 0,
        layer_b_source: 0,
        layer_m_source: 0,
        layer_c_source: 0,
        layer_1_status: LayerStatus::Variant,
        layer_2_status: LayerStatus::Variant,
        layer_3_status: LayerStatus::Variant,
        layer_4_status: LayerStatus::Variant,
        layer_a_status: LayerStatus::Variant,
        layer_b_status: LayerStatus::Variant,
        layer_m_status: LayerStatus::Variant,
        layer_c_status: LayerStatus::Variant,
        layer_1_track_id: TRACK_1_ID,
        layer_2_track_id: TRACK_2_ID,
        layer_3_track_id: 0,
        layer_4_track_id: 0,
        layer_a_track_id: 0,
        layer_b_track_id: 0,
        layer_m_track_id: 0,
        layer_c_track_id: 0,
        _reserved1: Default::default(),
        smpte_mode: 25,
        auto_master_mode: AutoMasterMode::Variant,
        _reserved2: Default::default(),
        app_specific: [0u8; 72],
        layer_1_name,
        layer_2_name,
        layer_3_name: [0u8; 16],
        layer_4_name: [0u8; 16],
        layer_a_name: [0u8; 16],
        layer_b_name: [0u8; 16],
        layer_m_name: [0u8; 16],
        layer_c_name: [0u8; 16],
    };
    (header.to_bytes().unwrap(), data.to_bytes().unwrap())
}

fn metrics_l1_bytes(config: &ApplicationConfig, seq: u8) -> (Vec<u8>, Vec<u8>) {
    let header = management_header(config, 200, seq);
    let data = MetricsData {
        data_type: 2,
        layer_id: LayerId::L1.as_packet_id(),
        _reserved0: Default::default(),
        layer_state: LayerState::Playing,
        _reserved1: Default::default(),
        sync_master: 1,
        _reserved2: Default::default(),
        beat_marker: 2,
        track_length: TRACK_1_LEN,
        current_position: TRACK_1_POS,
        speed: 32_768, // 100%
        _reserved3: Default::default(),
        beat_number: 64,
        _reserved4: Default::default(),
        bpm: TRACK_1_BPM,
        pitch_bend: 0,
        track_id: TRACK_1_ID,
    };
    (header.to_bytes().unwrap(), data.to_bytes().unwrap())
}

fn time_bytes(config: &ApplicationConfig, seq: u8) -> (Vec<u8>, Vec<u8>) {
    let header = management_header(config, 254, seq);
    let zero_tc = LayerTimecode {
        smpte_mode: 25,
        state: 0,
        hours: 0,
        minutes: 0,
        seconds: 0,
        frames: 0,
    };
    let data = TimePacketData {
        l1_time: TRACK_1_POS,
        l2_time: 0,
        l3_time: 0,
        l4_time: 0,
        la_time: 0,
        lb_time: 0,
        lm_time: 0,
        lc_time: 0,
        l1_total_time: TRACK_1_LEN,
        l2_total_time: 0,
        l3_total_time: 0,
        l4_total_time: 0,
        la_total_time: 0,
        lb_total_time: 0,
        lm_total_time: 0,
        lc_total_time: 0,
        l1_beat_marker: 2,
        l2_beat_marker: 0,
        l3_beat_marker: 0,
        l4_beat_marker: 0,
        la_beat_marker: 0,
        lb_beat_marker: 0,
        lm_beat_marker: 0,
        lc_beat_marker: 0,
        l1_layer_state: LayerState::Playing,
        l2_layer_state: LayerState::Idle,
        l3_layer_state: LayerState::Idle,
        l4_layer_state: LayerState::Idle,
        la_layer_state: LayerState::Idle,
        lb_layer_state: LayerState::Idle,
        lm_layer_state: LayerState::Idle,
        lc_layer_state: LayerState::Idle,
        _reserved0: Default::default(),
        smpte_mode: 25,
        l1_timecode: zero_tc,
        l2_timecode: zero_tc,
        l3_timecode: zero_tc,
        l4_timecode: zero_tc,
        la_timecode: zero_tc,
        lb_timecode: zero_tc,
        lm_timecode: zero_tc,
        lc_timecode: zero_tc,
        l1_on_air: 200,
        l2_on_air: 0,
        l3_on_air: 0,
        l4_on_air: 0,
        la_on_air: 0,
        lb_on_air: 0,
        lm_on_air: 0,
        lc_on_air: 0,
    };
    (header.to_bytes().unwrap(), data.to_bytes().unwrap())
}

fn mixer_bytes(config: &ApplicationConfig, seq: u8) -> (Vec<u8>, Vec<u8>) {
    let header = management_header(config, 200, seq);
    let zero_ch = MixerChannel {
        source_select: 0,
        audio_level: 0,
        fader_level: 0,
        trim_level: 128,
        comp_level: 0,
        eq_hi_level: 128,
        eq_hi_mid_level: 128,
        eq_low_mid_level: 128,
        eq_low_level: 128,
        filter_color: 128,
        send: 0,
        cue_a: 0,
        cue_b: 0,
        crossfader_assign: 0,
        _reserved: [0u8; 10],
    };
    let ch1 = MixerChannel {
        fader_level: CH1_FADER,
        audio_level: CH1_FADER,
        ..zero_ch
    };
    let ch2 = MixerChannel {
        fader_level: CH2_FADER,
        ..zero_ch
    };
    let data = MixerData {
        data_type: 150,
        mixer_id: 1,
        mixer_type: 1,
        _reserved0: Default::default(),
        _reserved1: Default::default(),
        mixer_name: *b"DJM-900NXS2\0\0\0\0\0",
        _reserved2: Default::default(),
        _reserved3: Default::default(),
        mic_eq_hi: 128,
        mic_eq_low: 128,
        master_audio_level: MASTER_FADER,
        master_fader_level: MASTER_FADER,
        _reserved4: Default::default(),
        link_cue_a: 0,
        link_cue_b: 0,
        master_filter: 128,
        _reserved5: Default::default(),
        master_cue_a: 0,
        master_cue_b: 0,
        _reserved6: Default::default(),
        master_isolator_on_off: 0,
        master_isolator_hi: 128,
        master_isolator_mid: 128,
        master_isolator_low: 128,
        _reserved7: Default::default(),
        filter_hpf: 0,
        filter_lpf: 255,
        filter_resonance: 0,
        _reserved8: Default::default(),
        send_fx_effect: 0,
        send_fx_ext_1: 0,
        send_fx_ext_2: 0,
        send_fx_master_mix: 0,
        send_fx_size_feedback: 0,
        send_fx_time: 0,
        send_fx_hpf: 0,
        send_fx_level: 0,
        send_return_3_source_select: 0,
        send_return_3_type: 0,
        send_return_3_on_off: 0,
        send_return_3_level: 0,
        _reserved9: Default::default(),
        channel_fader_curve: 0,
        cross_fader_curve: 0,
        cross_fader: CROSSFADER,
        beat_fx_on_off: 0,
        beat_fx_level_depth: 0,
        beat_fx_channel_select: 0,
        beat_fx_select: 0,
        beat_fx_freq_hi: 0,
        beat_fx_freq_mid: 0,
        beat_fx_freq_low: 0,
        headphones_pre_eq: 0,
        headphones_a_level: 0,
        headphones_a_mix: 0,
        headphones_b_level: 0,
        headphones_b_mix: 0,
        booth_level: 128,
        booth_eq_hi: 128,
        booth_eq_low: 128,
        _reserved10: [0u8; 10],
        channels: [ch1, ch2, zero_ch, zero_ch, zero_ch, zero_ch],
    };
    (header.to_bytes().unwrap(), data.to_bytes().unwrap())
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_cdj_play_session() {
    let _ = env_logger::try_init();
    let dest: SocketAddr = "127.0.0.1:60000".parse().unwrap();

    let config = ApplicationConfig::default();
    let client = TCNetClient::new(config);
    // Give the tokio runtime time to bind all listening sockets before sending.
    sleep(Duration::from_millis(500));

    let sock = UdpSocket::bind("0.0.0.0:0").expect("bind fake CDJ socket");
    eprintln!("fake CDJ bound to {}", sock.local_addr().unwrap());
    let cfg = fake_config();

    // 1. OptIn — register the fake CDJ as a foreign node
    let (h, d) = opt_in_bytes(&cfg, 0);
    send_packet(&sock, dest, h, d);
    sleep(Duration::from_millis(150));

    // 2. Status — load tracks on L1 and L2
    let (h, d) = status_bytes(&cfg, 1);
    send_packet(&sock, dest, h, d);

    // 3. Metrics — L1 is playing
    let (h, d) = metrics_l1_bytes(&cfg, 2);
    send_packet(&sock, dest, h, d);

    // 4. Time — current positions for all layers
    let (h, d) = time_bytes(&cfg, 3);
    send_packet(&sock, dest, h, d);

    // 5. Mixer state
    let (h, d) = mixer_bytes(&cfg, 4);
    send_packet(&sock, dest, h, d);

    // Allow the dj_controller_task to drain packets and write to triple buffer
    sleep(Duration::from_millis(200));

    client._runtime.block_on(async {
        let state = client.dispatcher.state.write().await;
        trace!(
            "The following nodes were discovered {:?}",
            state.discovered_nodes
        );
    });
    let mut view = client
        .get_any_controller_view()
        .expect("DjControllerView not available — no DJ packets received?");

    // Give dj_controller_task time to drain the forwarded packets and write state.
    sleep(Duration::from_millis(100));

    // --- Layer assertions (Vec is ordered by LayerId::ALL: L1=0, L2=1, …) ---
    {
        let layers = view.get_layers();
        assert_eq!(layers.len(), 8, "expected 8 layer slots");

        // L1 — playing Track One
        let l1 = &layers[0];
        assert_eq!(l1.track_id, TRACK_1_ID, "L1 track_id");
        assert_eq!(l1.state, LayerState::Playing, "L1 state");
        assert_eq!(l1.bpm.0, TRACK_1_BPM, "L1 bpm");
        assert_eq!(l1.position_ms, TRACK_1_POS, "L1 position_ms");
        assert_eq!(l1.current_time_ms, TRACK_1_POS, "L1 current_time_ms");
        assert_eq!(l1.total_time_ms, TRACK_1_LEN, "L1 total_time_ms");
        assert_eq!(l1.track_length_ms, TRACK_1_LEN, "L1 track_length_ms");
        assert!(l1.sync_master, "L1 sync_master");
        assert_eq!(l1.on_air, 200, "L1 on_air");
        assert!(l1.name.starts_with("Track One"), "L1 name: {:?}", l1.name);
        assert_eq!(l1.source, 1, "L1 source");

        // L2 — loaded but idle
        let l2 = &layers[1];
        assert_eq!(l2.track_id, TRACK_2_ID, "L2 track_id");
        assert_eq!(l2.state, LayerState::Idle, "L2 state");
        assert!(l2.name.starts_with("Track Two"), "L2 name: {:?}", l2.name);
        assert_eq!(l2.source, 2, "L2 source");

        // L3–LC — nothing loaded
        for i in 2..8 {
            assert_eq!(layers[i].track_id, 0, "layer {} should have no track", i);
        }
    }

    // --- Mixer assertions ---
    {
        let mixer = view.get_mixer();
        assert_eq!(mixer.master_fader_level, MASTER_FADER, "master_fader_level");
        assert_eq!(mixer.crossfader, CROSSFADER, "crossfader");
        assert_eq!(mixer.channels[0].fader_level, CH1_FADER, "ch1 fader");
        assert_eq!(mixer.channels[1].fader_level, CH2_FADER, "ch2 fader");
        assert_eq!(mixer.mixer_id, 1, "mixer_id");
    }
}

// ---------------------------------------------------------------------------
// REQUEST / response round-trip test
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_request_response() {
    use crate::active_node::TrackMeta;
    use crate::node::tcnet_packet::Packet;

    let _ = env_logger::try_init();

    // Start a Master-type TCNetClient on 127.0.0.1.
    let mut node_config = ApplicationConfig::default();
    node_config.node_type = NodeType::Master;
    let client = TCNetClient::new(node_config);
    sleep(Duration::from_millis(400));

    // Load a track so all response_data slots are populated.
    let mut node = client.create_active_node();
    node.load_track(
        LayerId::L1,
        TrackMeta {
            title: "Test Track".into(),
            artist: "Test Artist".into(),
            duration_ms: 60_000,
            bpm: 120.0,
            track_id: 1,
        },
    )
    .expect("load_track failed");
    // Trigger mixer data so last_mixer is populated.
    node.set_master_fader(200).ok();
    sleep(Duration::from_millis(150));

    // Bind the "viewer" socket and register it with the dispatcher via OptIn.
    let viewer = UdpSocket::bind("127.0.0.1:0").expect("bind viewer");
    viewer
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let viewer_port = viewer.local_addr().unwrap().port();

    let dest: SocketAddr = "127.0.0.1:60000".parse().unwrap();
    let viewer_cfg = ApplicationConfig {
        node_type: NodeType::Slave,
        address: SocketAddrV4::new(Ipv4Addr::LOCALHOST, viewer_port),
        ..node_config
    };
    let (h, d) = opt_in_bytes(&viewer_cfg, 200);
    send_packet(&viewer, dest, h, d);
    sleep(Duration::from_millis(150));

    // Receive REQUEST-response packets, skipping proactive broadcasts
    // (OptIn/OptOut/Time/Status/Meta/Metrics — the latter two are now
    // re-broadcast once a track is loaded, see
    // test_metrics_broadcast_for_stopped_layer). The skip set excludes
    // whatever variant the test is currently asserting on.
    let recv_packets = |expected: usize, want: RequestDataType|
        -> Vec<crate::node::tcnet_packet::Data> {
        use crate::node::tcnet_packet::Data as D;
        let matches_want = |d: &D| match (d, want) {
            (D::Metrics(_), RequestDataType::MetricsData) => true,
            (D::Meta(_), RequestDataType::MetaData) => true,
            (D::BeatGrid(_), RequestDataType::BeatGridData) => true,
            (D::Cue(_), RequestDataType::CueData) => true,
            (D::SmallWaveform(_), RequestDataType::SmallWaveformData) => true,
            (D::BigWaveform(_), RequestDataType::LargeWaveformData) => true,
            (D::ArtworkFile(_), RequestDataType::LowResArtworkFile) => true,
            (D::Mixer(_), RequestDataType::MixerData) => true,
            _ => false,
        };
        let mut collected = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match viewer.recv_from(&mut buf) {
                Ok((size, _)) => {
                    if let Ok(pkt) = Packet::deserialize_packet(&buf[..size]) {
                        if matches_want(&pkt.data) {
                            collected.push(pkt.data);
                            if collected.len() >= expected {
                                break;
                            }
                        }
                    }
                }
                Err(_) => break, // read timeout
            }
        }
        collected
    };

    let send_req = |data_type: RequestDataType| {
        let header = management_header(&viewer_cfg, 20, 201);
        let req = RequestData {
            data_type,
            layer: LayerId::L1,
        };
        let payload = [header.to_bytes().unwrap(), req.to_bytes().unwrap()].concat();
        viewer.send_to(&payload, dest).expect("send request");
    };

    // MetricsData
    send_req(RequestDataType::MetricsData);
    let pkts = recv_packets(1, RequestDataType::MetricsData);
    assert!(
        matches!(
            pkts.first(),
            Some(crate::node::tcnet_packet::Data::Metrics(_))
        ),
        "MetricsData response: {:?}",
        pkts
    );

    // MetaData
    send_req(RequestDataType::MetaData);
    let pkts = recv_packets(1, RequestDataType::MetaData);
    assert!(
        matches!(pkts.first(), Some(crate::node::tcnet_packet::Data::Meta(_))),
        "MetaData response: {:?}",
        pkts
    );

    // BeatGridData
    send_req(RequestDataType::BeatGridData);
    let pkts = recv_packets(1, RequestDataType::BeatGridData);
    assert!(
        matches!(
            pkts.first(),
            Some(crate::node::tcnet_packet::Data::BeatGrid(_))
        ),
        "BeatGridData response: {:?}",
        pkts
    );

    // CueData
    send_req(RequestDataType::CueData);
    let pkts = recv_packets(1, RequestDataType::CueData);
    assert!(
        matches!(pkts.first(), Some(crate::node::tcnet_packet::Data::Cue(_))),
        "CueData response: {:?}",
        pkts
    );

    // SmallWaveformData
    send_req(RequestDataType::SmallWaveformData);
    let pkts = recv_packets(1, RequestDataType::SmallWaveformData);
    assert!(
        matches!(
            pkts.first(),
            Some(crate::node::tcnet_packet::Data::SmallWaveform(_))
        ),
        "SmallWaveformData response: {:?}",
        pkts
    );

    // LargeWaveformData
    send_req(RequestDataType::LargeWaveformData);
    let pkts = recv_packets(1, RequestDataType::LargeWaveformData);
    assert!(
        matches!(
            pkts.first(),
            Some(crate::node::tcnet_packet::Data::BigWaveform(_))
        ),
        "LargeWaveformData response: {:?}",
        pkts
    );

    // LowResArtworkFile (empty placeholder)
    send_req(RequestDataType::LowResArtworkFile);
    let pkts = recv_packets(1, RequestDataType::LowResArtworkFile);
    assert!(
        matches!(
            pkts.first(),
            Some(crate::node::tcnet_packet::Data::ArtworkFile(_))
        ),
        "LowResArtworkFile response: {:?}",
        pkts
    );

    // MixerData
    send_req(RequestDataType::MixerData);
    let pkts = recv_packets(1, RequestDataType::MixerData);
    assert!(
        matches!(
            pkts.first(),
            Some(crate::node::tcnet_packet::Data::Mixer(_))
        ),
        "MixerData response: {:?}",
        pkts
    );
}

// ---------------------------------------------------------------------------
// Waveform routing regression test
// ---------------------------------------------------------------------------
// Verifies two properties of the routing fix:
// 1. SmallWaveform responses are sent from port 60000 (broadcast_socket), not 65023.
// 2. A REQUEST arriving from a different source port than the OptIn registration
//    is still routed to the correct discovered node address (IP-based fallback).
//
// Uses raw sockets instead of a second TCNetClient to avoid the port-60000
// conflict that prevents two in-process clients from doing broadcast discovery.

#[test]
#[serial]
fn test_waveform_routing() {
    use crate::active_node::TrackMeta;
    use crate::node::tcnet_packet::Packet;

    let _ = env_logger::try_init();

    // Master node
    let mut master_cfg = ApplicationConfig::default();
    master_cfg.node_type = NodeType::Master;
    let master_client = TCNetClient::new(master_cfg);
    let mut active = master_client.create_active_node();
    sleep(Duration::from_millis(400));

    active
        .load_track(
            LayerId::L1,
            TrackMeta {
                title: "Test Track".into(),
                artist: "Test Artist".into(),
                duration_ms: 60_000,
                bpm: 120.0,
                track_id: 99,
            },
        )
        .expect("load_track failed");
    sleep(Duration::from_millis(150));

    let dest: SocketAddr = "127.0.0.1:60000".parse().unwrap();

    // Socket A: registers the viewer address via OptIn.
    // The master will unicast responses to this address.
    let viewer = UdpSocket::bind("127.0.0.1:0").expect("bind viewer");
    viewer
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let viewer_port = viewer.local_addr().unwrap().port();

    let viewer_cfg = ApplicationConfig {
        node_type: NodeType::Slave,
        address: SocketAddrV4::new(Ipv4Addr::LOCALHOST, viewer_port),
        ..master_cfg
    };
    let (h, d) = opt_in_bytes(&viewer_cfg, 0);
    send_packet(&viewer, dest, h, d);
    sleep(Duration::from_millis(150));

    // Socket B: sends the REQUEST from a *different* source port to exercise
    // the IP-based routing fallback in the REQUEST handler.
    // The response must still arrive at viewer (socket A) because that is the
    // registered node address, not at socket B.
    let requester = UdpSocket::bind("127.0.0.1:0").expect("bind requester");
    let header = management_header(&viewer_cfg, 20, 201);
    let req = RequestData {
        data_type: RequestDataType::SmallWaveformData,
        layer: LayerId::L1,
    };
    let payload = [header.to_bytes().unwrap(), req.to_bytes().unwrap()].concat();
    requester.send_to(&payload, dest).expect("send REQUEST");

    // Receive the SmallWaveform response on the viewer socket (the registered address).
    // Skip proactive broadcasts (OptIn/OptOut/Time/Status) as in test_request_response.
    let mut buf = [0u8; 8192];
    let mut got_waveform = false;
    let mut response_src_port = 0u16;
    loop {
        match viewer.recv_from(&mut buf) {
            Ok((size, src)) => {
                if let Ok(pkt) = Packet::deserialize_packet(&buf[..size]) {
                    match pkt.data {
                        crate::node::tcnet_packet::Data::OptIn(_)
                        | crate::node::tcnet_packet::Data::OptOut(_)
                        | crate::node::tcnet_packet::Data::Time(_)
                        | crate::node::tcnet_packet::Data::Status(_) => {}
                        crate::node::tcnet_packet::Data::SmallWaveform(_) => {
                            response_src_port = src.port();
                            got_waveform = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(_) => break,
        }
    }

    assert!(
        got_waveform,
        "SmallWaveform response not received — IP-based routing fallback broken"
    );
    assert_eq!(
        response_src_port, 60000,
        "SmallWaveform came from port {} instead of 60000 — send task must use broadcast_socket",
        response_src_port
    );
}

// ---------------------------------------------------------------------------
// Loopback discovery test (DJ Link Bridge scenario)
// ---------------------------------------------------------------------------
// Verifies that when another process already holds port 60000 (simulating
// DJ Link Bridge), our client still broadcasts OptIn on the loopback network
// and can then receive and route DJ packets from the "remote" node.

#[test]
#[serial]
fn test_loopback_discovery() {
    use crate::node::tcnet_packet::{Data, Packet};

    let _ = env_logger::try_init();

    // 1. Occupy port 60000 — mimics DJ Link Bridge holding the canonical port.
    let dj_link = UdpSocket::bind("127.0.0.1:60000").expect("bind port 60000");
    dj_link
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();

    // 2. Create viewer — its broadcast socket falls back to a non-60000 port.
    let client = TCNetClient::new(ApplicationConfig::default());
    sleep(Duration::from_millis(500));

    // 3. Wait for viewer's loopback OptIn broadcast (Step 1 fix).
    let mut buf = [0u8; 8192];
    let (size, _) = dj_link
        .recv_from(&mut buf)
        .expect("no loopback OptIn received — broadcast fix missing");

    // 4. Parse to get viewer's unicast listener port from the OptIn payload.
    let pkt = Packet::deserialize_packet(&buf[..size]).expect("parse failed");
    let listener_port = match &pkt.data {
        Data::OptIn(o) => o.node_listener_port,
        _ => panic!("expected OptIn, got {:?}", pkt.data),
    };

    // 5. Send fake OptIn from DJ Link's socket → viewer's unicast port.
    let viewer_unicast: SocketAddr = format!("127.0.0.1:{}", listener_port).parse().unwrap();
    let dj_cfg = ApplicationConfig {
        node_type: NodeType::Master,
        address: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 60_000),
        ..ApplicationConfig::default()
    };
    let (h, d) = opt_in_bytes(&dj_cfg, 0);
    send_packet(&dj_link, viewer_unicast, h, d);
    sleep(Duration::from_millis(100));

    // 6. Send fake Metrics → triggers DjController creation via the existing is_dj_packet path.
    let (h, d) = metrics_l1_bytes(&dj_cfg, 1);
    send_packet(&dj_link, viewer_unicast, h, d);
    sleep(Duration::from_millis(200));

    // 7. DjController must now exist with the correct BPM.
    let mut view = client
        .get_any_controller_view()
        .expect("DjControllerView not available after Metrics — loopback discovery broken");
    assert_eq!(view.get_layers()[0].bpm.0, TRACK_1_BPM, "BPM mismatch");
}

// ---------------------------------------------------------------------------
// Metrics-for-stopped-layer regression test
// ---------------------------------------------------------------------------
// Reproduces the bug where a slave that joined *after* a track was loaded but
// never started playing would see `track_length_ms = 0` and `bpm = 0` for
// that deck — because the active node only re-broadcast MetricsData for
// `state.is_playing()` layers, and MetaData (the only repeat-broadcast that
// covered loaded-but-stopped decks) does not carry track length or BPM.
//
// The fix re-broadcasts MetricsData for every layer with `track_id != 0`
// regardless of play state. This test pins that behaviour so a future
// "optimisation" can't quietly bring the bug back.

#[test]
#[serial]
fn test_metrics_broadcast_for_stopped_layer() {
    use crate::active_node::TrackMeta;
    use crate::node::tcnet_packet::{Data, Packet};

    let _ = env_logger::try_init();

    // Master node — loads a track on L1 but does NOT play it.
    let mut master_cfg = ApplicationConfig::default();
    master_cfg.node_type = NodeType::Master;
    let master_client = TCNetClient::new(master_cfg);
    let mut active = master_client.create_active_node();
    sleep(Duration::from_millis(400));

    const STOPPED_BPM: f32 = 128.0;
    const STOPPED_LEN_MS: u32 = 240_000;
    const STOPPED_TRACK_ID: u32 = 77;
    active
        .load_track(
            LayerId::L1,
            TrackMeta {
                title: "Stopped Track".into(),
                artist: "Test Artist".into(),
                duration_ms: STOPPED_LEN_MS,
                bpm: STOPPED_BPM,
                track_id: STOPPED_TRACK_ID,
            },
        )
        .expect("load_track failed");
    // No `play()` call — the deck stays in `LayerState::Stopped`.

    // Slave viewer registers via OptIn AFTER the load. This mirrors the
    // real-world scenario where luchs starts after the simulator already
    // has a track loaded.
    let viewer = UdpSocket::bind("127.0.0.1:0").expect("bind viewer");
    viewer
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let viewer_port = viewer.local_addr().unwrap().port();
    let viewer_cfg = ApplicationConfig {
        node_type: NodeType::Slave,
        address: SocketAddrV4::new(Ipv4Addr::LOCALHOST, viewer_port),
        ..master_cfg
    };
    let dest: SocketAddr = "127.0.0.1:60000".parse().unwrap();
    let (h, d) = opt_in_bytes(&viewer_cfg, 0);
    send_packet(&viewer, dest, h, d);

    // Listen for at least one Metrics packet for L1. The metrics_tick fires
    // every 50 ms, so a 1-second window is comfortably long enough.
    let mut buf = [0u8; 8192];
    let mut metrics: Option<MetricsData> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while metrics.is_none() && std::time::Instant::now() < deadline {
        match viewer.recv_from(&mut buf) {
            Ok((size, _)) => {
                if let Ok(pkt) = Packet::deserialize_packet(&buf[..size]) {
                    if let Data::Metrics(m) = pkt.data {
                        if m.layer_id == LayerId::L1.as_packet_id() {
                            metrics = Some(m);
                        }
                    }
                }
            }
            Err(_) => break,
        }
    }

    let m = metrics.expect(
        "No MetricsData received for stopped L1 layer — \
         active node is filtering by is_playing() again and slaves that \
         join after the load will never learn track_length / bpm.",
    );
    assert_eq!(m.layer_state, LayerState::Stopped, "layer must be Stopped");
    assert_eq!(
        m.track_length, STOPPED_LEN_MS,
        "track_length must be carried in the broadcast Metrics"
    );
    assert_eq!(
        m.bpm,
        (STOPPED_BPM * 100.0) as u32,
        "bpm must be carried in the broadcast Metrics"
    );
    assert_eq!(m.track_id, STOPPED_TRACK_ID, "track_id must match the loaded track");
}
