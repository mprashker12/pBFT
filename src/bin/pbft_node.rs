use pbft::node::Node;
use pbft::net_sender::NetSender;
use pbft::messages::{Message, PrePrepare, NetSenderCommand};

use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::collections::HashMap;

use tokio::sync::mpsc;


#[tokio::main]
async fn main() -> std::io::Result<()> {

    let (tx, rx) = mpsc::channel::<NetSenderCommand>(32);

    let mut node = Node {
        tx_net_sender: tx,
    };

    let node_fut = tokio::spawn(async move {
        node.spawn().await;
    });

    let mut net_sender = NetSender {
        command_receiver: rx,
        open_connections: HashMap::new(),
    };


    let net_sender_fut = tokio::spawn( async move {
        net_sender.spawn().await;
    });


    node_fut.await.unwrap();
    net_sender_fut.await.unwrap();
    
    Ok(())
}