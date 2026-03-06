use std::net::Ipv4Addr;
use std::sync::Arc;
use crate::node::{send_message, ApplicationConfig, Dispatcher};
use crate::node::tcnet_packet_serde::{Data, ManagementHeader, NodeId};

pub struct ApplicationNode {
    pub(crate) node: Arc<Dispatcher>,
    pub(crate) config: ApplicationConfig,
    pub(crate) incoming_handler: Box<dyn Fn(ManagementHeader, Data)>,
}

impl ApplicationNode {
    pub fn send_message(&self, address: Ipv4Addr, node_id: NodeId, data: Data) {
        send_message(&self, address, node_id, data);
    }
    pub(crate) fn handle_incoming_message(&self, header: ManagementHeader, data: Data) {
        (*self.incoming_handler)(header, data);
    }
}