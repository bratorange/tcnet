use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use crate::node::{start_node, DynamicNodeState, Node, NodeConfig};

pub mod node;

const SPEC_MAJOR_VERSION: u8 = 3;
const SPEC_MINOR_VERSION: u8 = 6;

pub struct TCNetClient{
    runtime: Runtime,
    node: Arc<Node>,
}

impl TCNetClient{
    pub fn new(bind_address: Ipv4Addr) -> Self{
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("tcnet")
            .enable_all()
            .build().expect("Could not start tokio runtime");

        let mut node = Arc::new(Node {
            config: NodeConfig::default(),
            bind_address,
            state: Arc::new(RwLock::new(DynamicNodeState::default())),
        });

        runtime.spawn(start_node(node.clone()));
        Self{runtime, node}
    }
}