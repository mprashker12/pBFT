use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::{Key, NodeId, Value};

use sha2::{Digest, Sha256};
use ed25519_dalek::{Keypair, PublicKey, SecretKey};

/// Messages which are communicated between nodes in the network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    IdentifierMessage(Identifier),
    PrePrepareMessage(PrePrepare),
    PrepareMessage(Prepare),
    CommitMessage(Commit),
    ClientRequestMessage(ClientRequest),
    ClientResponseMessage(ClientResponse),
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        let mut serialized_message = serde_json::to_string(self).unwrap();
        serialized_message.push('\n');
        serialized_message.into_bytes()
    }

    pub fn get_id(&self) -> Option<NodeId> {
        match self.clone() {
            Message::IdentifierMessage(identifier) => {Some(identifier.id)}
            Message::PrePrepareMessage(pre_prepare) => {Some(pre_prepare.id)}
            Message::PrepareMessage(prepare) => {Some(prepare.id)}
            Message::CommitMessage(commit) => {Some(commit.id)}
            Message::ClientRequestMessage(_) => {None}
            Message::ClientResponseMessage(client_response) => {Some(client_response.id)}
        }
    }

    pub fn get_signature(&self) -> Option<usize> {
        match self.clone() {
            Message::IdentifierMessage(_) => {None}
            Message::PrePrepareMessage(pre_prepare) => {Some(pre_prepare.signature)}
            Message::PrepareMessage(prepare) => {Some(prepare.signature)}
            Message::CommitMessage(commit) => {Some(commit.signature)}
            Message::ClientRequestMessage(_) => {None}
            Message::ClientResponseMessage(client_response) => {Some(client_response.signature)}
        }
    }

    pub fn is_properly_signed_by(&self, pub_key: &PublicKey) -> bool {
        if self.get_signature().is_none() {return true;}
        let signature = self.get_signature().unwrap();

        true
    }
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
pub struct Identifier {
    pub id: NodeId,
    pub pub_key_vec: Vec<u8>,
}

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Prepare {
    pub id: NodeId,
    pub view: usize,
    pub seq_num: usize,
    /// Hash of the associated client request
    pub digest: Vec<u8>,
    pub signature: usize,
}

impl Prepare {
    // does this prepare message correspond to the pre_prepare message
    pub fn corresponds_to(&self, pre_prepare: &PrePrepare) -> bool {
        if self.view != pre_prepare.view {
            return false;
        }
        if self.seq_num != pre_prepare.seq_num {
            return false;
        }
        if self.digest != pre_prepare.digest {
            return false;
        }
        true
    }
}

// Note that the Prepare message does not include the client_request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Commit {
    pub id: NodeId,
    pub view: usize,
    pub seq_num: usize,
    pub digest: Vec<u8>,
    pub signature: usize,
}

impl Commit {
    // does this commit message correspond to the prepare message
    pub fn corresponds_to(&self, prepare: &Prepare) -> bool {
        if self.view != prepare.view {
            return false;
        }
        if self.seq_num != prepare.seq_num {
            return false;
        }
        if self.digest != prepare.digest {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewChange {
    pub id: NodeId,
    pub new_view: usize,
    pub seq_num: usize,
    pub checkpoint_messages: Vec<Prepare>,
    pub signature: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ClientResponse {
    pub id: NodeId,
    pub time_stamp: usize,
    pub key: Key,
    pub value: Value,
    pub success: bool,
    pub signature: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CheckPoint {
    pub id: NodeId,
    pub committed_seq_num: usize,
    pub state_digest: Vec<u8>,
    pub signature: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ClientRequest {
    pub respond_addr: SocketAddr,
    pub time_stamp: usize,
    pub key: Key,
    pub value: Option<Value>,
}

impl ClientRequest {
    pub fn hash(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(self.respond_addr.to_string().as_bytes());
        hasher.update(self.time_stamp.to_le_bytes());
        hasher.update(self.key.as_bytes());
        if self.value.is_some() {
            hasher.update(self.value.unwrap().to_le_bytes());
        }
        let result: &[u8] = &hasher.finalize();
        result.to_vec()
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
