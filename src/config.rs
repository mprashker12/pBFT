use std::collections::HashMap;
use std::net::SocketAddr;

use crate::NodeId;


#[derive(Clone, Default)]
pub struct Config {
    /// Number of nodes in the system
    pub num_nodes: usize,

    pub num_faulty: usize,
    /// Address which each node is listening on
    pub peer_addrs: HashMap<NodeId, SocketAddr>,
    
    pub request_timeout: std::time::Duration,

    /// KEY ASSUMPTION: A node can not be down for more than an entire checkpoint interval
    /// otherwise, the node will fall behind forever
    pub checkpoint_frequency: usize,
}
