use std::net::IpAddr;



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