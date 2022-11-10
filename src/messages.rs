use std::net::SocketAddr;

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Message {
    PrePrepareMessage(PrePrepare),
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NetSenderCommand {
    Send {
        peer_addr: SocketAddr,
        message: Message
    }
}



#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PrePrepare {
    pub view: usize,
    pub seq_num: usize,
    pub digest: usize /* TODO: Make this some hash */
}

