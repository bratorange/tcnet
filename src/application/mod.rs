use crate::node::dispatcher::Dispatcher;
use crate::node::tcnet_packet::{Data, Packet};
use crate::node::ApplicationConfig;
use kanal::{Receiver, Sender};
use std::net::Ipv4Addr;
use std::sync::Arc;
use crate::node::tcnet_packet_serde::NodeId;

pub mod dj_controller_view;
mod active_dj_controller;

// just pass on data for now
pub struct ApplicationMessage {
    pub target_addr: Ipv4Addr,
    pub target_node_id: NodeId,
    pub data: Data,
}

pub struct ApplicationNode {
    pub dispatcher: Arc<Dispatcher>,
    pub config: ApplicationConfig,
    pub incoming_tx: Sender<Packet>,
    pub outgoing_rx: Receiver<ApplicationMessage>,
}