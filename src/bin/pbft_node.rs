use pbft::messages::{Message, NetSenderCommand, PrePrepare};
use pbft::net_sender::NetSender;
use pbft::node::Node;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // TODO: Read this as a command line arg
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8022);

    let (tx, rx) = mpsc::channel::<NetSenderCommand>(32);

    let mut node = Node::new(listen_addr, tx);
    let node_fut = tokio::spawn(async move {
        node.run().await;
    });

    let mut net_sender = NetSender {
        command_receiver: rx,
        open_connections: HashMap::new(),
    };
    let net_sender_fut = tokio::spawn(async move {
        net_sender.spawn().await;
    });

    //await all futures (Perhaps make this a tokio select)
    node_fut.await.unwrap();
    net_sender_fut.await.unwrap();

    Ok(())
}
