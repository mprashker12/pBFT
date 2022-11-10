use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

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

/// Commands used internally in channels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NetSenderCommand {
    Send {
        peer_addr: SocketAddr,
        message: Message,
    },
}
