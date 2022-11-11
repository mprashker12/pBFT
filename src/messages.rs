use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::NodeId;

/// Messages which are communicated between nodes in the network
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Message {
    PrePrepareMessage(PrePrepare),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PrePrepare {
    pub view: usize,
    pub seq_num: usize,
    pub digest: usize, /* TODO: Make this some hash */
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Prepare {
    pub view: usize,
    pub seq_num: usize,
    pub digest: usize, /* TODO: Make this some hash */
    pub id: NodeId,
}



