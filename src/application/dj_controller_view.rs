use crate::node::tcnet_packet_serde::AsciiString;

pub struct Layer{
    source: u8,
    status: u8,
    track_id: u32,
    name: AsciiString<16>,
}

// TODO how should the layers be represented? maybe as a deku serde struct?
pub struct ControllerStatus{
    layers: Vec<Layer>,
}

pub struct DJControllerView {
    status: ControllerStatus,
}

impl DJControllerView {
    pub fn new() -> Self {
        Self {
            status: ControllerStatus{
                layers: vec![],
            }
        }
    }
}