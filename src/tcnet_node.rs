use std::net::IpAddr;
use deku::{DekuRead, DekuWrite};

struct Node {
    name: String,
    bind_address: IpAddr,
}

impl Node {
    pub fn init(name: String, bind_address: IpAddr) -> Self {
        let ret = Self {
            name,
            bind_address,
        };

        ret
    }
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
struct NodeConfig {
    need_authentication: bool,
    supports_tcncm: bool,
    supports_tcnasdps: bool,
    dnd: bool,
}