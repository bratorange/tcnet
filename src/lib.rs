use crate::node::dispatcher::{start_node, Dispatcher};
use crate::node::dj_controller::OutgoingRequest;
use crate::node::response_data::{ResponseDataStore, SharedResponseData};
use crate::node::tcnet_packet::Data;
use crate::node::tcnet_packet_serde::NodeId;
use crate::node::{DynamicNodeState, ForeignNode};
use std::net::SocketAddrV4;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

pub mod node;
pub mod active_node;
#[cfg(feature = "simulator")]
pub mod simulator;
mod dj_controller_view;
#[cfg(test)]
mod tests;

pub use active_node::ActiveDJNode;
pub use dj_controller_view::DjControllerView;
pub use node::dj_controller::{
    ChannelSnapshot, DjControllerState, LayerSnapshot, MixerSnapshot, TimeoutError,
};
pub use node::tcnet_packet_serde::NodeType;
pub use node::ApplicationConfig;

const SPEC_MAJOR_VERSION: u8 = 3;
const SPEC_MINOR_VERSION: u8 = 6;

/// Snapshot of a discovered foreign node, readable via `TCNetClient::active_nodes()`.
#[derive(Clone, Debug)]
pub struct ForeignNodeInfo {
    pub address: SocketAddrV4,
    pub last_seen: u64,
    pub node_id: NodeId,
    pub has_dj_controller: bool,
}

impl From<&ForeignNode> for ForeignNodeInfo {
    fn from(n: &ForeignNode) -> Self {
        ForeignNodeInfo {
            address: n.address,
            last_seen: n.last_seen,
            node_id: n.config.node_id,
            has_dj_controller: n.dj_controller.is_some(),
        }
    }
}

pub struct TCNetClient {
    _runtime: Runtime,
    dispatcher: Arc<Dispatcher>,
    nodes_output: triple_buffer::Output<Vec<ForeignNodeInfo>>,
    cached_nodes: Vec<ForeignNodeInfo>,
    active_broadcast_tx: kanal::Sender<Data>,
    active_slave_unicast_tx: kanal::Sender<Data>,
    active_time_tx: kanal::Sender<Data>,
    response_data: SharedResponseData,
}

impl TCNetClient {
    pub fn new(node_config: ApplicationConfig) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("tcnet")
            .enable_all()
            .build()
            .expect("Could not start tokio runtime");

        let (outgoing_tx, outgoing_rx) = kanal::bounded::<OutgoingRequest>(256);
        let (nodes_input, nodes_output) =
            triple_buffer::triple_buffer(&Vec::<ForeignNodeInfo>::new());
        let (active_broadcast_tx, active_broadcast_rx) = kanal::bounded::<Data>(512);
        let (active_slave_unicast_tx, active_slave_unicast_rx) = kanal::bounded::<Data>(512);
        let (active_time_tx, active_time_rx) = kanal::bounded::<Data>(512);
        let response_data: SharedResponseData = Arc::new(Mutex::new(ResponseDataStore::default()));

        let dispatcher = Arc::new(Dispatcher {
            node_config,
            bind_address: node_config.address,
            state: Arc::new(RwLock::new(DynamicNodeState::default())),
            outgoing_tx,
            outgoing_rx,
            nodes_buf_input: Arc::new(Mutex::new(nodes_input)),
            active_broadcast_rx,
            active_slave_unicast_rx,
            active_time_rx,
            response_data: response_data.clone(),
        });

        runtime.spawn(start_node(dispatcher.clone()));

        Self {
            _runtime: runtime,
            dispatcher,
            nodes_output,
            cached_nodes: Vec::new(),
            active_broadcast_tx,
            active_slave_unicast_tx,
            active_time_tx,
            response_data,
        }
    }

    /// Returns the latest known set of active foreign nodes.
    pub fn active_nodes(&mut self) -> &[ForeignNodeInfo] {
        self.cached_nodes = self.nodes_output.read().clone();
        &self.cached_nodes
    }

    /// Returns a `DjControllerView` for the node at `addr` if DJ-type packets
    /// have been received from it.
    pub fn get_controller_view(&self, addr: SocketAddrV4) -> Option<DjControllerView> {
        self._runtime.block_on(async {
            let mut state = self.dispatcher.state.write().await;
            let ctrl = state.discovered_nodes.get_mut(&addr)?
                .dj_controller.as_mut()?;
            let buf = ctrl.buf_output.take()?;
            Some(DjControllerView::new(buf, ctrl.request_tx.clone()))
        })
    }

    /// Returns a `DjControllerView` for the first discovered DJ controller node.
    pub fn get_any_controller_view(&self) -> Option<DjControllerView> {
        self._runtime.block_on(async {
            let mut state = self.dispatcher.state.write().await;
            for node in state.discovered_nodes.values_mut() {
                if let Some(ctrl) = node.dj_controller.as_mut() {
                    if let Some(buf) = ctrl.buf_output.take() {
                        return Some(DjControllerView::new(buf, ctrl.request_tx.clone()));
                    }
                }
            }
            None
        })
    }

    /// Creates an `ActiveDJNode` that broadcasts this node's state over TCNet.
    pub fn create_active_node(&self) -> ActiveDJNode {
        ActiveDJNode::new(
            self.active_broadcast_tx.clone(),
            self.active_slave_unicast_tx.clone(),
            self.active_time_tx.clone(),
            self.response_data.clone(),
            &self._runtime,
        )
    }
}
