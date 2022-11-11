use pbft::node::Node;
use pbft::Result;


use std::net::{IpAddr, Ipv4Addr, SocketAddr};



#[tokio::main]
async fn main() -> Result<()> {
    // TODO: Read this as a command line arg
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8022);


    let mut node = Node::new(listen_addr);
    let node_fut = tokio::spawn(async move {
        node.run().await;
    });


    node_fut.await?;
    Ok(())
}
