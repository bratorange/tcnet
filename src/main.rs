use clap::Parser;
use std::net::Ipv4Addr;
use std::thread::sleep;
use std::time::Duration;
use tcnet::tcnet_node::Node;

#[derive(Parser)]
struct Args {
    binding_ip: Ipv4Addr,
}
fn main() {
    env_logger::init();
    let args = Args::parse();
    let binding_ip = args.binding_ip;
    Node::run(binding_ip).expect("Could not start TCNet node");
}