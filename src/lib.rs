use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use crate::node::{start_node, DynamicNodeState, Dispatcher, ApplicationConfig};
use crate::node::application::ApplicationNode;
use crate::node::tcnet_packet_serde::{Data, ManagementHeader, NodeName, NodeType};

pub mod node;

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

        let mut node = Arc::new(Dispatcher {
            config: ApplicationConfig::default(),
            bind_address,
            state: Arc::new(RwLock::new(DynamicNodeState::default())),
        });

        runtime.spawn(start_node(node.clone()));
        Self{runtime, node}
    }
    pub async fn add_application(
        &self,
        node_name: NodeName,
        node_type: NodeType,
        
        incoming_handler: Box<dyn Fn(ManagementHeader, Data)>
    ) -> ApplicationNode {
        ApplicationNode { node: self.node.clone(), config: self.node.config.clone(), incoming_handler }
    }
}