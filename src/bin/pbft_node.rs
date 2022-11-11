use pbft::config::Config;
use pbft::node::Node;
use pbft::Result;


use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, collections::HashMap};



#[tokio::main]
async fn main() -> Result<()> {
    
    // TODO: We will eventually read the config from the command line

    let mut listen_addrs = HashMap::new();
    listen_addrs.insert(0, SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8060));
    listen_addrs.insert(1, SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8061));
    listen_addrs.insert(2, SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8062));

    let config = Config {
        num_nodes : 3,
        listen_addrs,
    };


    let mut node = Node::new(0, config);
    let node_fut = tokio::spawn(async move {
        node.run().await;
    });


    node_fut.await?;
    Ok(())
}
