mod dj_controller_view;

use std::net::Ipv4Addr;
use std::sync::Arc;
use crate::node::{send_message, ApplicationConfig, Dispatcher};
use crate::node::tcnet_packet_serde::{Data, ManagementHeader, NodeId};

pub trait Application {
    fn handle_message(&self, header: &ManagementHeader, data: &Data);
    fn start(&mut self);
}

pub struct ApplicationNode {
    pub(crate) dispatcher: Arc<Dispatcher>,
    pub(crate) config: ApplicationConfig,
    pub(crate) application: Box<dyn Application + Send + Sync>,
}

impl ApplicationNode {
    pub fn send_message(&self, address: Ipv4Addr, node_id: NodeId, data: Data) {
        send_message(&self, address, node_id, data);
    }
    pub(crate) fn handle_incoming_message(&self, header: &ManagementHeader, data: &Data) {
        self.application.handle_message(header, data);
    }
}