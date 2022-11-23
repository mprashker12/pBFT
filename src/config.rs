use std::collections::HashMap;
use std::net::SocketAddr;

use crate::NodeId;

#[derive(Clone, Default)]
pub struct Config {
    /// Number of nodes in the system
    pub num_nodes: usize,
    /// How many faulty nodes the system can tolerate
    pub num_faulty: usize,
    /// Address which each node is listening on
    pub peer_addrs: HashMap<NodeId, SocketAddr>,
    /// How long we wait after receiving a pre-prepare request
    /// which we have not yet executed before initiating a view-change
    pub request_timeout: std::time::Duration,

    /// How many requests we see in between stable checkpoints
    pub checkpoint_frequency: usize,
}
