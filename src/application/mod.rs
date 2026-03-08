mod dj_controller_view;

use crate::node::tcnet_packet::Packet;
use crate::node::ApplicationConfig;
use kanal::{Receiver, Sender};
use std::sync::Arc;
use crate::node::dispatcher::Dispatcher;

// just pass on data for now
pub type ApplicationMessage = Packet;

pub struct ApplicationNode {
    pub(crate) dispatcher: Arc<Dispatcher>,
    pub(crate) config: ApplicationConfig,
    pub(crate) incoming_tx: Sender<ApplicationMessage>,
    pub(crate) outgoing_rx: Receiver<ApplicationMessage>,
}