use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use crate::node::{DynamicNodeState, ForeignNode};
use crate::node::dispatcher::{Dispatcher, start_node};
use crate::node::dj_controller::OutgoingRequest;
use crate::node::tcnet_packet_serde::NodeId;

pub mod node;
mod dj_controller_view;
#[cfg(test)]
mod tests;

pub use dj_controller_view::DjControllerView;
pub use node::dj_controller::{
    ChannelSnapshot, DjControllerState, LayerSnapshot, MixerSnapshot, TimeoutError,
};
pub use node::ApplicationConfig;

const SPEC_MAJOR_VERSION: u8 = 3;
const SPEC_MINOR_VERSION: u8 = 6;

/// Snapshot of a discovered foreign node, readable via `TCNetClient::active_nodes()`.
#[derive(Clone, Debug)]
pub struct ForeignNodeInfo {
    pub address: Ipv4Addr,
    pub last_seen: u64,
    pub node_ids: Vec<NodeId>,
    pub has_dj_controller: bool,
}

impl From<&ForeignNode> for ForeignNodeInfo {
    fn from(n: &ForeignNode) -> Self {
        ForeignNodeInfo {
            address: n.address,
            last_seen: n.last_seen,
            node_ids: n.applications.keys().copied().collect(),
            has_dj_controller: n.dj_controller.is_some(),
        }
    }
}

pub struct TCNetClient {
    _runtime: Runtime,
    dispatcher: Arc<Dispatcher>,
    nodes_output: triple_buffer::Output<Vec<ForeignNodeInfo>>,
    cached_nodes: Vec<ForeignNodeInfo>,
}

impl TCNetClient {
    pub fn new(bind_address: Ipv4Addr, node_config: ApplicationConfig) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("tcnet")
            .enable_all()
            .build()
            .expect("Could not start tokio runtime");

        let (outgoing_tx, outgoing_rx) = kanal::bounded::<OutgoingRequest>(256);
        let (nodes_input, nodes_output) =
            triple_buffer::triple_buffer(&Vec::<ForeignNodeInfo>::new());

        let dispatcher = Arc::new(Dispatcher {
            node_config,
            unicast_port: node_config.unicast_port,
            bind_address,
            state: Arc::new(RwLock::new(DynamicNodeState::default())),
            outgoing_tx,
            outgoing_rx,
            nodes_buf_input: Arc::new(Mutex::new(nodes_input)),
        });

        runtime.spawn(start_node(dispatcher.clone()));

        Self {
            _runtime: runtime,
            dispatcher,
            nodes_output,
            cached_nodes: Vec::new(),
        }
    }

    /// Returns the latest known set of active foreign nodes.
    ///
    /// This is a lock-free read from the triple buffer.
    pub fn active_nodes(&mut self) -> &[ForeignNodeInfo] {
        self.cached_nodes = self.nodes_output.read().clone();
        &self.cached_nodes
    }

    /// Returns a `DjControllerView` for the node at `addr` if DJ-type packets
    /// have been received from it.
    ///
    /// This moves the triple buffer read handle into the returned view, so it
    /// can only be called once per address.
    pub fn get_controller_view(&self, addr: Ipv4Addr) -> Option<DjControllerView> {
        self._runtime.block_on(async {
            let mut state = self.dispatcher.state.write().await;
            let ctrl = state.discovered_nodes.get_mut(&addr)?
                .dj_controller.as_mut()?;
            let buf = ctrl.buf_output.take()?;
            Some(DjControllerView::new(buf, ctrl.request_tx.clone()))
        })
    }
}