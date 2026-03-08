use crate::application::{ApplicationMessage, ApplicationNode};
use crate::node::tcnet_packet_serde::Data;
use crate::node::{start_node, Dispatcher, DynamicNodeState};
use kanal::Receiver;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

pub mod node;
mod application;

const SPEC_MAJOR_VERSION: u8 = 3;
const SPEC_MINOR_VERSION: u8 = 6;

pub struct TCNetClient{
    runtime: Runtime,
    node: Arc<Dispatcher>,
}

impl TCNetClient{
    pub fn new(bind_address: Ipv4Addr) -> Self{
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("tcnet")
            .enable_all()
            .build().expect("Could not start tokio runtime");

        let dispatcher = Arc::new(Dispatcher {
            application_nodes: Arc::new(RwLock::default()),
            unicast_port: 65_023,
            bind_address,
            state: Arc::new(RwLock::new(DynamicNodeState::default())),
        });

        runtime.spawn(start_node(dispatcher.clone()));
        Self{runtime, node: dispatcher }
    }
    pub async fn add_application(
        &self,
        application_config: node::ApplicationConfig,
    ) -> (Receiver<ApplicationMessage>, kanal::Sender<ApplicationMessage>) {
        let (incoming_tx, incoming_rx) = kanal::bounded(100);
        let (outgoing_tx, outgoing_rx) = kanal::bounded(100);
        let application_node = ApplicationNode { dispatcher: self.node.clone(), config: application_config, incoming_tx, outgoing_rx };
        self.node.application_nodes.write().await.insert(application_node.config.node_id, application_node);
        (incoming_rx, outgoing_tx)
    }
}