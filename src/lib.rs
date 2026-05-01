use crate::node::dispatcher::add_application;
use crate::node::tcnet_packet_serde::{NodeOptions, NodeType};
use crate::node::{ApplicationConfig, DynamicNodeState};
use node::dispatcher::{Dispatcher, start_node};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use crate::application::dj_controller_view::DjControllerView;

mod application;
pub mod node;

const SPEC_MAJOR_VERSION: u8 = 3;
const SPEC_MINOR_VERSION: u8 = 6;

pub struct TCNetClient {
    _runtime: Runtime,
    dispatcher: Arc<Dispatcher>,
}

impl TCNetClient {
    pub fn new(bind_address: Ipv4Addr) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("tcnet")
            .enable_all()
            .build()
            .expect("Could not start tokio runtime");

        let dispatcher = Arc::new(Dispatcher {
            application_nodes: Arc::new(RwLock::default()),
            unicast_port: 65_023,
            bind_address,
            state: Arc::new(RwLock::new(DynamicNodeState::default())),
        });
        runtime.spawn(start_node(dispatcher.clone()));
        let dispatcher_clone = dispatcher.clone();
        let mut temp_test_app = runtime.block_on(async move {
            DjControllerView::new(add_application(
                dispatcher_clone,
                ApplicationConfig {
                    node_id: 0,
                    node_type: NodeType::Slave,
                    vendor_name: into_ascii!("NoVendor________"),
                    application_name: into_ascii!("NoApplication___"),
                    application_major_version: 0,
                    application_minor_version: 1,
                    application_bug_version: 0,
                    node_name: into_ascii!("DJView  "),
                    node_options: NodeOptions::empty(),
                    unicast_port: 65_023,
                },
            ).await)
        });
        runtime.spawn(async move {
            loop {
                temp_test_app.process_available();
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });
        Self {
            _runtime: runtime,
            dispatcher,
        }
    }
}
