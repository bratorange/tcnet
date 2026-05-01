use std::time::Duration;
use kanal::Receiver;
use crate::application::ApplicationMessage;
use crate::node::tcnet_packet::management_header;
use crate::node::tcnet_packet_serde::{AsciiString, Data};

pub struct Layer{
    source: u8,
    status: u8,
    track_id: u32,
    name: AsciiString<16>,
}

// TODO how should the layers be represented? maybe as a deku serde struct?
pub struct ControllerStatus{
    layer_1_source: u8,
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
                layer_1_source: 0,
            },
            rx,
            tx,
        }
    }

    pub fn process_packages(&mut self){
        if let Ok(packet) = self.rx.recv_timeout(Duration::from_millis(10)){
            match packet.data {
                Data::Status(status_data) => {
                    self.status.layer_1_source = status_data.layer_1_source;
                },
                _ => {},
            }
        }
    }
}