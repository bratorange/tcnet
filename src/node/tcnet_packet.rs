use crate::into_ascii;
use crate::node::dispatcher::timestamp_micros;
use crate::node::tcnet_packet::Data::*;
use crate::node::{ApplicationConfig, DynamicNodeState};
use crate::protocol::*;
use deku::prelude::Writer;
use deku::{DekuContainerRead, DekuContainerWrite, DekuError, DekuWrite, DekuWriter};
use std::fmt::Debug;
use std::io::{Seek, Write};
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Clone, DekuWrite)]
pub struct Packet {
    pub(crate) header: ManagementHeader,
    pub(crate) data: Data,
}

#[derive(Debug)]
pub enum SerdeError {
    InvalidHeader(DekuError),
    InvalidData(DekuError),
    MessageTypeNotImplemented,
}

impl Packet {
    pub fn deserialize_packet(bytes: &[u8]) -> Result<Self, SerdeError> {
        let (remaining, header) =
            ManagementHeader::from_bytes((bytes, 0)).map_err(SerdeError::InvalidHeader)?;
        let packet_type = header.message_type;

        let data = match packet_type {
            2 => {
                let (_, inner) =
                    OptInData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                OptIn(inner)
            }
            3 => {
                let (_, inner) =
                    OptOutData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                OptOut(inner)
            }
            5 => {
                let (_, inner) =
                    StatusData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                Status(inner)
            }
            10 => {
                let (_, inner) =
                    TimeSyncData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                TimeSync(inner)
            }
            13 => {
                let (_, inner) = ErrorNotificationData::from_bytes(remaining)
                    .map_err(SerdeError::InvalidData)?;
                ErrorNotification(inner)
            }
            20 => {
                let (_, inner) =
                    RequestData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                Request(inner)
            }
            // Type 30 and 213 are both AppSpecific (different transport: 30
            // unicasts on Target-Node-Port, 213 broadcasts on 60000). The
            // payload layout is identical so we route both into the same
            // variant.
            30 | 213 => {
                let (_, inner) =
                    AppSpecificData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                AppSpecific(inner)
            }
            101 => {
                let (_, inner) =
                    ControlData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                Control(inner)
            }
            128 => {
                let (_, inner) =
                    TextData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                Text(inner)
            }
            132 => {
                let (_, inner) =
                    KeyboardData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                Keyboard(inner)
            }
            200 => {
                // Data type is the first byte of the remaining buffer
                let data_type =
                    *remaining
                        .0
                        .first()
                        .ok_or(SerdeError::InvalidData(DekuError::Parse(
                            "missing data type byte".into(),
                        )))?;
                match data_type {
                    2 => {
                        let (_, inner) =
                            MetricsData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                        Metrics(inner)
                    }
                    4 => {
                        let (_, inner) =
                            MetaData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                        Meta(inner)
                    }
                    8 => {
                        let (_, inner) = BeatGridHeader::from_bytes(remaining)
                            .map_err(SerdeError::InvalidData)?;
                        BeatGrid(inner)
                    }
                    12 => {
                        let (_, inner) =
                            CueData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                        Cue(inner)
                    }
                    16 => {
                        let (_, inner) = SmallWaveformData::from_bytes(remaining)
                            .map_err(SerdeError::InvalidData)?;
                        SmallWaveform(inner)
                    }
                    32 => {
                        let (_, inner) = BigWaveformData::from_bytes(remaining)
                            .map_err(SerdeError::InvalidData)?;
                        BigWaveform(inner)
                    }
                    128 => {
                        let (_, inner) = ArtworkFileData::from_bytes(remaining)
                            .map_err(SerdeError::InvalidData)?;
                        ArtworkFile(inner)
                    }
                    150 => {
                        let (_, inner) =
                            MixerData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                        Mixer(inner)
                    }
                    _ => return Err(SerdeError::MessageTypeNotImplemented),
                }
            }
            254 => {
                let (_, inner) =
                    TimePacketData::from_bytes(remaining).map_err(SerdeError::InvalidData)?;
                Time(inner)
            }
            _ => return Err(SerdeError::MessageTypeNotImplemented),
        };
        Ok(Self { header, data })
    }
}

impl Debug for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Packet\n {{ Header: {:?}\n\nData:\n {:?} }}",
            self.header, self.data
        )
    }
}

pub(crate) fn node_config_from_opt_in(
    src_addr: Ipv4Addr,
    header: &ManagementHeader,
    data: &OptInData,
) -> ApplicationConfig {
    ApplicationConfig {
        node_id: header.node_id,
        node_type: header.node_type,
        vendor_name: data.vendor_name,
        application_name: data.application,
        application_major_version: data.application_major_version,
        application_minor_version: data.application_minor_version,
        application_bug_version: data.application_bug_version,
        node_name: header.node_name,
        node_options: header.node_options,
        address: SocketAddrV4::new(src_addr, data.node_listener_port),
    }
}

/// Build a best-effort [`ApplicationConfig`] from a [`ManagementHeader`]
/// when we have no matching OptIn yet — used by `is_dj_packet` routing for
/// nodes whose first packet was a DJ-class one. `node_id`, `node_type`,
/// `node_name` and `node_options` carry across faithfully; the OptIn-only
/// fields (vendor / app name / version) fall back to defaults until the
/// real OptIn arrives and overwrites the entry.
pub(crate) fn config_from_header(
    header: &ManagementHeader,
    src_addr: Ipv4Addr,
) -> ApplicationConfig {
    let defaults = ApplicationConfig::default();
    ApplicationConfig {
        node_id: header.node_id,
        node_type: header.node_type,
        vendor_name: defaults.vendor_name,
        application_name: defaults.application_name,
        application_major_version: 0,
        application_minor_version: 0,
        application_bug_version: 0,
        node_name: header.node_name,
        node_options: header.node_options,
        address: SocketAddrV4::new(src_addr, 65_023),
    }
}

pub(crate) fn management_header(
    app_config: &ApplicationConfig,
    message_type: u8,
    seq: u8,
) -> ManagementHeader {
    ManagementHeader {
        node_id: app_config.node_id,
        protocol_version_major: 3,
        protocol_version_minor: 6,
        _header: into_ascii!("TCN"),
        message_type,
        node_name: app_config.node_name,
        seq,
        node_type: app_config.node_type,
        node_options: app_config.node_options,
        timestamp: timestamp_micros(),
    }
}

pub(crate) fn opt_in_packet(
    app_config: &ApplicationConfig,
    node_state: &DynamicNodeState,
    seq: u8,
) -> Result<Vec<u8>, DekuError> {
    let header = management_header(app_config, 2, seq);
    let data = OptInData {
        node_count: (node_state.discovered_nodes.len() + 1) as u16,
        node_listener_port: app_config.address.port(),
        uptime: node_state.uptime,
        _reserved0: Default::default(),
        vendor_name: app_config.vendor_name,
        application: app_config.application_name,
        application_major_version: app_config.application_major_version,
        application_minor_version: app_config.application_minor_version,
        application_bug_version: app_config.application_bug_version,
        _reserved1: Default::default(),
    };
    let ret = [header.to_bytes()?, data.to_bytes()?].concat();
    debug_assert!(ret.len() == 68);
    Ok(ret)
}

impl Data {
    pub fn message_type_id(&self) -> (u8, Option<u8>) {
        match self {
            OptIn(_) => (2, None),
            OptOut(_) => (3, None),
            Status(_) => (5, None),
            TimeSync(_) => (10, None),
            ErrorNotification(_) => (13, None),
            Request(_) => (20, None),
            AppSpecific(_) => (30, None),
            Control(_) => (101, None),
            Text(_) => (128, None),
            Keyboard(_) => (132, None),

            Metrics(_) => (200, Some(2)),
            Meta(_) => (200, Some(4)),
            BeatGrid(_) => (200, Some(8)),
            Cue(_) => (200, Some(12)),
            SmallWaveform(_) => (200, Some(16)),
            BigWaveform(_) => (200, Some(32)),
            Mixer(_) => (200, Some(150)),

            ArtworkFile(_) => (200, Some(128)),
            // AppSpecific(_) => 254, None)
            Time(_) => (254, None),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Data {
    OptIn(OptInData),
    OptOut(OptOutData),
    Status(StatusData),
    TimeSync(TimeSyncData),
    ErrorNotification(ErrorNotificationData),
    Request(RequestData),
    AppSpecific(AppSpecificData),
    Control(ControlData),
    Text(TextData),
    Keyboard(KeyboardData),
    Metrics(MetricsData),
    Meta(MetaData),
    BeatGrid(BeatGridHeader),
    Cue(CueData),
    SmallWaveform(SmallWaveformData),
    BigWaveform(BigWaveformData),
    Mixer(MixerData),
    ArtworkFile(ArtworkFileData),
    Time(TimePacketData),
}

impl DekuWriter for Data {
    fn to_writer<W: Write + Seek>(&self, writer: &mut Writer<W>, ctx: ()) -> Result<(), DekuError> {
        match self {
            OptIn(data) => data.to_writer(writer, ctx),
            OptOut(data) => data.to_writer(writer, ctx),
            Status(data) => data.to_writer(writer, ctx),
            TimeSync(data) => data.to_writer(writer, ctx),
            ErrorNotification(data) => data.to_writer(writer, ctx),
            Request(data) => data.to_writer(writer, ctx),
            AppSpecific(data) => data.to_writer(writer, ctx),
            Control(data) => data.to_writer(writer, ctx),
            Text(data) => data.to_writer(writer, ctx),
            Keyboard(data) => data.to_writer(writer, ctx),
            Metrics(data) => data.to_writer(writer, ctx),
            Meta(data) => data.to_writer(writer, ctx),
            BeatGrid(data) => data.to_writer(writer, ctx),
            Cue(data) => data.to_writer(writer, ctx),
            SmallWaveform(data) => data.to_writer(writer, ctx),
            BigWaveform(data) => data.to_writer(writer, ctx),
            Mixer(data) => data.to_writer(writer, ctx),
            ArtworkFile(data) => data.to_writer(writer, ctx),
            Time(data) => data.to_writer(writer, ctx),
        }
    }
}
