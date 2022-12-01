use pbft::config::Config;
use pbft::consensus::Consensus;
use pbft::messages::{ConsensusCommand, NodeCommand};
use pbft::node::Node;
use pbft::Result;

use std::str::FromStr;

use ed25519_dalek::Keypair;
use rand::rngs::OsRng;

use tokio::sync::mpsc::channel;

use std::{collections::HashMap, env, net::SocketAddr};

#[tokio::main]
async fn main() -> Result<()> {
    // TODO: We will eventually read the config from the command line
    let args: Vec<String> = env::args().collect();
    let mut index = 1;
    let num_nodes = args[index].parse::<usize>().unwrap();
    let mut peer_addrs = HashMap::new();
    index += 1;
    for id in 0..num_nodes {
        let addr = args[index].clone();
        peer_addrs.insert(id, SocketAddr::from_str(addr.as_str()).unwrap());
        index += 1;
    }
    let id = args[index].parse::<usize>().unwrap();
    index += 1;

    let mut is_equivocator = false;
    if index < args.len() {
        let byzantine_flag = args[index].clone();
        is_equivocator = byzantine_flag.as_str().eq("b");
    }

    let num_faulty: usize = (num_nodes - 1) / 3;

    let config = Config {
        num_nodes,
        num_faulty,
        peer_addrs,
        request_timeout: std::time::Duration::from_secs(5),
        rebroadcast_timeout: std::time::Duration::from_secs(6),
        identity_broadcast_interval: std::time::Duration::from_secs(3),
        checkpoint_frequency: 10,
        is_equivocator,
    };

    let (tx_consensus, rx_consensus) = channel::<ConsensusCommand>(32);
    let (tx_node, rx_node) = channel::<NodeCommand>(32);

    // generate a keypair for the node
    let mut rng = OsRng {};
    let keypair: Keypair = Keypair::generate(&mut rng);
    let keypair_bytes = keypair.to_bytes().to_vec();

    let mut node = Node::new(
        id,
        config.clone(),
        keypair_bytes.clone(),
        keypair.public,
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
        keypair_bytes.clone(),
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
