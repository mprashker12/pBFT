use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::NodeId;

/// Messages which are communicated between nodes in the network
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Message {
    PrePrepareMessage(PrePrepare),
    PrepareMessage(Prepare),
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        let mut serialized_message = serde_json::to_string(self).unwrap();
        serialized_message.push('\n');
        serialized_message.into_bytes()
    }
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
