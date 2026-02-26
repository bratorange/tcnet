use crate::tcnet_package_serde::{Data, ManagementHeader, OptInData, StatusData};
use deku::{DekuContainerRead, DekuError};
use std::fmt::Debug;
use log::debug;

pub struct Package
{
    header: ManagementHeader,
    data: Data,
}

#[derive(Debug)]
pub enum SerdeError{
    InvalidHeader(DekuError),
    InvalidData(DekuError),
    MessageTypeNotImplemented,
}

impl Package {
    pub fn deserialize_package(bytes: &[u8]) -> Result<Self, SerdeError > {
        // Each package must be at least 21 bytes long to be able to contain at least a header
        let (remaining, header) = ManagementHeader::from_bytes((bytes, 0)).map_err(|x| SerdeError::InvalidHeader(x))?;
        let package_type = header.message_type;

        debug!("header: {:?}", header);
        let data = match package_type {
            5 => {
                let (_, inner) = StatusData::from_bytes(remaining).map_err(|x| SerdeError::InvalidData(x))?;
                Data::Status(inner)
            }
            2 => {
                let (_, inner) = OptInData::from_bytes(remaining).map_err(|x| SerdeError::InvalidData(x))?;
                Data::OptIn(inner)
            }
            _ => {return Err(SerdeError::MessageTypeNotImplemented)},
        };
        Ok(Self{header, data})
    }
}

impl Debug for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Package\n {{ Header: {:?}\n\nData:\n {:?} }}", self.header, self.data)
    }
}