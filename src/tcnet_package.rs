use crate::tcnet_packet_serde::*;
use deku::{DekuContainerRead, DekuError};
use std::fmt::Debug;
use log::debug;

pub struct Package
{
    header: ManagementHeader,
    data: Data,
}

#[derive(Debug)]
pub enum SerdeError {
    InvalidHeader(DekuError),
    InvalidData(DekuError),
    MessageTypeNotImplemented,
}

impl Package {
    pub fn deserialize_package(bytes: &[u8]) -> Result<Self, SerdeError> {
        let (remaining, header) = ManagementHeader::from_bytes((bytes, 0))
            .map_err(|x| SerdeError::InvalidHeader(x))?;
        let package_type = header.message_type;

        debug!("header: {:?}", header);
        let data = match package_type {
            2 => {
                let (_, inner) = OptInData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::OptIn(inner)
            }
            3 => {
                let (_, inner) = OptOutData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::OptOut(inner)
            }
            5 => {
                let (_, inner) = StatusData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::Status(inner)
            }
            10 => {
                let (_, inner) = TimeSyncData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::TimeSync(inner)
            }
            13 => {
                let (_, inner) = ErrorNotificationData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::ErrorNotification(inner)
            }
            20 => {
                let (_, inner) = RequestData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::Request(inner)
            }
            30 => {
                let (_, inner) = AppSpecificData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::AppSpecific(inner)
            }
            101 => {
                let (_, inner) = ControlData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::Control(inner)
            }
            128 => {
                let (_, inner) = TextData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::Text(inner)
            }
            132 => {
                let (_, inner) = KeyboardData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::Keyboard(inner)
            }
            200 => {
                // Data type is the first byte of the remaining buffer
                let data_type = *remaining.0.first()
                    .ok_or(SerdeError::InvalidData(DekuError::Parse("missing data type byte".into())))?;
                match data_type {
                    2 => {
                        let (_, inner) = MetricsData::from_bytes(remaining)
                            .map_err(|x| SerdeError::InvalidData(x))?;
                        Data::Metrics(inner)
                    }
                    4 => {
                        let (_, inner) = MetaData::from_bytes(remaining)
                            .map_err(|x| SerdeError::InvalidData(x))?;
                        Data::Meta(inner)
                    }
                    8 => {
                        let (_, inner) = BeatGridHeader::from_bytes(remaining)
                            .map_err(|x| SerdeError::InvalidData(x))?;
                        Data::BeatGrid(inner)
                    }
                    12 => {
                        let (_, inner) = CueData::from_bytes(remaining)
                            .map_err(|x| SerdeError::InvalidData(x))?;
                        Data::Cue(inner)
                    }
                    16 => {
                        let (_, inner) = SmallWaveformData::from_bytes(remaining)
                            .map_err(|x| SerdeError::InvalidData(x))?;
                        Data::SmallWaveform(inner)
                    }
                    32 => {
                        let (_, inner) = BigWaveformData::from_bytes(remaining)
                            .map_err(|x| SerdeError::InvalidData(x))?;
                        Data::BigWaveform(inner)
                    }
                    150 => {
                        let (_, inner) = MixerData::from_bytes(remaining)
                            .map_err(|x| SerdeError::InvalidData(x))?;
                        Data::Mixer(inner)
                    }
                    _ => return Err(SerdeError::MessageTypeNotImplemented),
                }
            }
            204 => {
                let (_, inner) = ArtworkFileData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::ArtworkFile(inner)
            }
            254 => {
                let (_, inner) = TimePacketData::from_bytes(remaining)
                    .map_err(|x| SerdeError::InvalidData(x))?;
                Data::Time(inner)
            }
            _ => return Err(SerdeError::MessageTypeNotImplemented),
        };
        Ok(Self { header, data })
    }
}

impl Debug for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Package\n {{ Header: {:?}\n\nData:\n {:?} }}", self.header, self.data)
    }
}