use clap::Parser;
use std::net::Ipv4Addr;
use std::thread::sleep;
use std::time::Duration;
use tcnet::node::Node;

#[derive(Parser)]
struct Args {
    binding_ip: Ipv4Addr,
}
fn main() {
    env_logger::init();
    let args = Args::parse();
    let binding_ip = args.binding_ip;
    let rt = Node::start(binding_ip);
    loop {
        sleep(Duration::from_secs(10));
    }
}