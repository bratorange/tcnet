use crate::node::tcnet_packet::Packet;
use crate::node::ApplicationConfig;
use kanal::{Receiver, Sender};
use std::sync::Arc;
use crate::node::dispatcher::Dispatcher;
use crate::node::tcnet_packet_serde::NodeId;

pub mod dj_controller_view;
mod domain;
mod active_dj_controller;

// just pass on data for now
pub type ApplicationMessage = Packet;

pub struct ApplicationNode {
    pub dispatcher: Arc<Dispatcher>,
    pub config: ApplicationConfig,
    pub incoming_tx: Sender<ApplicationMessage>,
    pub outgoing_rx: Receiver<ApplicationMessage>,
}