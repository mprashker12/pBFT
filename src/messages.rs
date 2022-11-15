use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::{Key, NodeId, Value};

use sha2::{Digest, Sha256};

/// Messages which are communicated between nodes in the network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    PrePrepareMessage(PrePrepare),
    PrepareMessage(Prepare),
    CommitMessage(Commit),
    ClientRequestMessage(ClientRequest),
}

/// Commands to Consensus Engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusCommand {
    ProcessMessage(Message),
    MisdirectedClientRequest(ClientRequest),
    InitPrePrepare(ClientRequest),
    AcceptPrePrepare(PrePrepare),
    AcceptPrepare(Prepare),
    EnterCommit(Prepare),
    AcceptCommit(Commit),
    InitViewChange(ClientRequest),
    ApplyClientRequest(Commit),
}

/// Commands to Node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeCommand {
    SendMessageCommand(SendMessage),
    BroadCastMessageCommand(BroadCastMessage),
}

// Messages

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrePrepare {
    pub id: NodeId,
    pub view: usize,
    pub seq_num: usize,
    /// Hash of the associated client request
    pub digest: Vec<u8>,
    pub signature: usize, /*This will be a cryptograph signature of all of the above data */
    pub client_request: ClientRequest,
}

// Note that the Prepare message does not include the client_request
// because pre-prepare message already included it
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Prepare {
    pub id: NodeId,
    pub view: usize,
    pub seq_num: usize,
    /// Hash of the associated client request
    pub digest: Vec<u8>,
    pub signature: usize,
}

// Note that the Prepare message does not include the client_request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Commit {
    pub id: NodeId,
    pub view: usize,
    pub seq_num: usize,
    pub digest: Vec<u8>,
    pub signature: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewChange {
    pub id: NodeId,
    pub new_view: usize,
    pub seq_num: usize,
    pub checkpoint_messages: Vec<Prepare>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ClientRequest {
    pub respond_addr: SocketAddr,
    pub time_stamp: usize,
    pub key: Key,
    pub value: Value,
}

impl ClientRequest {
    pub fn hash(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(self.respond_addr.to_string().as_bytes());
        hasher.update(self.time_stamp.to_le_bytes());
        hasher.update(self.key.as_bytes());
        hasher.update(self.value.to_le_bytes());
        let result: &[u8] = &hasher.finalize();
        result.to_vec()
    }
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        let mut serialized_message = serde_json::to_string(self).unwrap();
        serialized_message.push('\n');
        serialized_message.into_bytes()
    }
}

// Commands to Node

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessage {
    pub destination: SocketAddr,
    pub message: Message,
}

// Commands to Node

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadCastMessage {
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnterCommit {
    pub view: usize,
    pub seq_num: usize,
    pub digest: usize, /* TODO: Make this some hash */
}
