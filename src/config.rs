use std::collections::HashMap;
use std::net::SocketAddr;

use crate::NodeId;

// part of the config will include public keys for each node in the system

#[derive(Clone, Default)]
pub struct Config {
    /// Number of nodes in the system
    pub num_nodes: usize,

    pub num_faulty: usize,
    /// Address which each node is listening on
    pub peer_addrs: HashMap<NodeId, SocketAddr>,

    pub request_timeout: std::time::Duration,

    pub checkpoint_frequency: usize,
}
