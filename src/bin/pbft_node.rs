use pbft::config::Config;
use pbft::consensus::Consensus;
use pbft::messages::{ConsensusCommand, NodeCommand};
use pbft::node::Node;
use pbft::Result;

use tokio::sync::mpsc::channel;

use std::{
    collections::HashMap,
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

#[tokio::main]
async fn main() -> Result<()> {
    // TODO: We will eventually read the config from the command line
    let args: Vec<String> = env::args().collect();
    let id = args[1].parse::<usize>().unwrap();

    let mut peer_addrs = HashMap::new();
    peer_addrs.insert(
        0,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8060),
    );
    peer_addrs.insert(
        1,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8061),
    );
    peer_addrs.insert(
        2,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8062),
    );

    let config = Config {
        num_nodes: 3,
        peer_addrs,
    };

    let (tx_consensus, rx_consensus) = channel::<ConsensusCommand>(32);
    let (tx_node, rx_node) = channel::<NodeCommand>(32);

    let mut node = Node::new(
        id,
        config.clone(),
        rx_node,
        tx_consensus.clone(),
        tx_node.clone(),
    );
    let node_fut = tokio::spawn(async move {
        node.spawn().await;
    });

    let mut consensus = Consensus::new(
        id,
        config.clone(),
        rx_consensus,
        tx_consensus.clone(),
        tx_node.clone(),
    );
    let consensus_fut = tokio::spawn(async move {
        consensus.spawn().await;
    });

    node_fut.await?;
    consensus_fut.await?;
    Ok(())
}
