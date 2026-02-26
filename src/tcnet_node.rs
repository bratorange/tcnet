use std::net::{IpAddr, SocketAddr};
use deku::{DekuRead, DekuWrite};
use tokio::net::UdpSocket;
use tokio::runtime;
use crate::tcnet_package::Package;

pub struct Node {
    name: String,
    bind_address: IpAddr,
}

impl Node {
    pub fn init(name: &str, bind_address: IpAddr) -> Result<Self, ()> {
        let ret = Self {
            name: name.to_string(),
            bind_address,
        };
        let socket_addr = SocketAddr::new(ret.bind_address, 60_000);
        let rt = runtime::Builder::new_current_thread()
            .enable_all()
            .build().expect("Could not create runtime");
        rt.block_on(async move {
            let listener = UdpSocket::bind(socket_addr).await.expect("Could not bind socket");

            loop {
                let mut buffer = [0; 1024];
                match listener.recv(&mut buffer).await {
                    Ok(size) => {
                        log::trace!("Received {} bytes from {}", size, listener.local_addr().unwrap());
                    },
                    Err(e) => {
                        log::error!("Network error: {}", e);
                    },
                };

                match Package::deserialize_package(&buffer) {
                    Ok(package) => {
                        log::trace!("Received package:");
                        log::trace!("{:?}", package);
                    },
                    Err(e) => {
                        log::error!("{:?}", e);
                    },
                }
            }
        });
        Ok(ret)
    }
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
struct NodeConfig {
    need_authentication: bool,
    supports_tcncm: bool,
    supports_tcnasdps: bool,
    dnd: bool,
}