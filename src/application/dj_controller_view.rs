use crate::application::Application;
use crate::node::tcnet_packet_serde::{AsciiString, Data, ManagementHeader};

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

impl Application for DJControllerView {
    fn handle_message(&self, header: &ManagementHeader, data: &Data) {
        match data {
            Data::Status(data) => {}
            _ => {}
        }
    }

    fn start(&mut self) {
        todo!()
    }
}

impl DJControllerView {

}