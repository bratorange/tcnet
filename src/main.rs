use std::net::{IpAddr, Ipv4Addr};
use tcnet::tcnet_node::Node;
use clap::{arg, Parser};
use tokio::runtime;

#[derive(Parser)]
struct Args {
    binding_ip: IpAddr,
}
fn main() {
    env_logger::init();
    let args = Args::parse();
    let binding_ip = args.binding_ip;
    let node = Node::init("TestNode", binding_ip);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}