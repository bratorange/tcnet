use kanal::Receiver;
use crate::application::ApplicationMessage;
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
    rx: Receiver<ApplicationMessage>,
    tx: kanal::Sender<ApplicationMessage>,
}

impl DJControllerView {
    pub fn new((rx, tx): (Receiver<ApplicationMessage>, kanal::Sender<ApplicationMessage>)) -> Self {
        Self {
            status: ControllerStatus{
                layers: vec![],
            },
            rx,
            tx,
        }
    }
}