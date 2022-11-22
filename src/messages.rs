use std::collections::HashMap;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::{Key, NodeId, Value};

use ed25519_dalek::{Digest, Sha512};
use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signature};

/// Messages which are communicated between nodes in the network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    IdentifierMessage(Identifier),
    PrePrepareMessage(PrePrepare),
    PrepareMessage(Prepare),
    CommitMessage(Commit),
    ViewChangeMessage(ViewChange),
    CheckPointMessage(CheckPoint),
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
            Message::IdentifierMessage(identifier) => Some(identifier.id),
            Message::PrePrepareMessage(pre_prepare) => Some(pre_prepare.id),
            Message::PrepareMessage(prepare) => Some(prepare.id),
            Message::CommitMessage(commit) => Some(commit.id),
            Message::ViewChangeMessage(view_change) => Some(view_change.id),
            Message::CheckPointMessage(check_point) => Some(check_point.id),
            Message::ClientResponseMessage(client_response) => Some(client_response.id),
            Message::ClientRequestMessage(_) => {
                // client request messages are not sent from nodes
                // so they have no associated ids
                None
            }
        }
    }

    /// Is this message propertly signed by the given public key
    pub fn is_properly_signed_by(&self, pub_key: &PublicKey) -> bool {
        match self.clone() {
            Message::IdentifierMessage(_) => {
                unreachable!()
            }
            Message::PrePrepareMessage(pre_prepare) => pre_prepare.is_properly_signed_by(pub_key),
            Message::PrepareMessage(prepare) => prepare.is_properly_signed_by(pub_key),
            Message::CommitMessage(commit) => commit.is_properly_signed_by(pub_key),
            _ => true,
        }
    }
}

// Messages

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identifier {
    pub id: NodeId,
    pub pub_key_vec: Vec<u8>,
}

// Note that the pre-prepare messages are the only messages which actually
// include the entire client request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PrePrepare {
    pub id: NodeId,
    pub view: usize,
    pub seq_num: usize,
    /// Hash of the associated client request
    pub client_request_digest: Vec<u8>,
    pub signature: Vec<u8>,
    pub client_request: ClientRequest,
}

impl PrePrepare {
    pub fn new_with_signature(
        key_pair_bytes: Vec<u8>,
        id: usize,
        view: usize,
        seq_num: usize,
        client_request: &ClientRequest,
    ) -> PrePrepare {
        let key_pair = Keypair::from_bytes(key_pair_bytes.as_slice()).unwrap();

        let mut pre_hashed = Sha512::new();
        pre_hashed.update(b"PrePrepare");
        pre_hashed.update(view.to_le_bytes());
        pre_hashed.update(seq_num.to_le_bytes());
        pre_hashed.update(client_request.digest().as_slice());

        let signature = key_pair.sign_prehashed(pre_hashed, None).unwrap();

        PrePrepare {
            id,
            view,
            seq_num,
            client_request_digest: client_request.digest(),
            signature: signature.to_bytes().to_vec(),
            client_request: client_request.clone(),
        }
    }

    pub fn is_properly_signed_by(&self, pub_key: &PublicKey) -> bool {
        let mut pre_hashed = Sha512::new();
        pre_hashed.update(b"PrePrepare");
        pre_hashed.update(self.view.to_le_bytes());
        pre_hashed.update(self.seq_num.to_le_bytes());
        pre_hashed.update(self.client_request.digest().as_slice());

        let signature = Signature::from_bytes(self.signature.as_slice()).unwrap();

        pub_key
            .verify_prehashed(pre_hashed, None, &signature)
            .is_ok()
    }
}

// Note that the Prepare message does not include the client_request
// because pre-prepare message already included it
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Prepare {
    pub id: NodeId,
    pub view: usize,
    pub seq_num: usize,
    /// Hash of the associated client request
    pub client_request_digest: Vec<u8>,
    pub signature: Vec<u8>,
}

impl Prepare {
    pub fn new_with_signature(
        key_pair_bytes: Vec<u8>,
        id: usize,
        view: usize,
        seq_num: usize,
        client_request: &ClientRequest,
    ) -> Prepare {
        let key_pair = Keypair::from_bytes(key_pair_bytes.as_slice()).unwrap();

        let mut pre_hashed = Sha512::new();
        pre_hashed.update(b"Prepare");
        pre_hashed.update(view.to_le_bytes());
        pre_hashed.update(seq_num.to_le_bytes());
        pre_hashed.update(client_request.digest().as_slice());

        let signature = key_pair.sign_prehashed(pre_hashed, None).unwrap();

        Prepare {
            id,
            view,
            seq_num,
            client_request_digest: client_request.digest(),
            signature: signature.to_bytes().to_vec(),
        }
    }

    pub fn is_properly_signed_by(&self, pub_key: &PublicKey) -> bool {
        let mut pre_hashed = Sha512::new();
        pre_hashed.update(b"Prepare");
        pre_hashed.update(self.view.to_le_bytes());
        pre_hashed.update(self.seq_num.to_le_bytes());
        pre_hashed.update(self.client_request_digest.as_slice());

        let signature = Signature::from_bytes(self.signature.as_slice()).unwrap();

        pub_key
            .verify_prehashed(pre_hashed, None, &signature)
            .is_ok()
    }

    // does this prepare message correspond to the pre_prepare message
    pub fn corresponds_to(&self, pre_prepare: &PrePrepare) -> bool {
        if self.view != pre_prepare.view {
            return false;
        }
        if self.seq_num != pre_prepare.seq_num {
            return false;
        }
        if self.client_request_digest != pre_prepare.client_request_digest {
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
    pub client_request_digest: Vec<u8>,
    pub signature: Vec<u8>,
}

impl Commit {
    pub fn new_with_signature(
        key_pair_bytes: Vec<u8>,
        id: usize,
        view: usize,
        seq_num: usize,
        client_request_digest: Vec<u8>,
    ) -> Commit {
        let key_pair = Keypair::from_bytes(key_pair_bytes.as_slice()).unwrap();

        let mut pre_hashed = Sha512::new();
        pre_hashed.update(b"Commit");
        pre_hashed.update(view.to_le_bytes());
        pre_hashed.update(seq_num.to_le_bytes());
        pre_hashed.update(client_request_digest.as_slice());

        let signature = key_pair.sign_prehashed(pre_hashed, None).unwrap();

        Commit {
            id,
            view,
            seq_num,
            client_request_digest,
            signature: signature.to_bytes().to_vec(),
        }
    }

    pub fn is_properly_signed_by(&self, pub_key: &PublicKey) -> bool {
        let mut pre_hashed = Sha512::new();
        pre_hashed.update(b"Commit");
        pre_hashed.update(self.view.to_le_bytes());
        pre_hashed.update(self.seq_num.to_le_bytes());
        pre_hashed.update(self.client_request_digest.as_slice());

        let signature = Signature::from_bytes(self.signature.as_slice()).unwrap();

        pub_key
            .verify_prehashed(pre_hashed, None, &signature)
            .is_ok()
    }

    /// Does this commit message correspond to the prepare message
    pub fn corresponds_to(&self, prepare: &Prepare) -> bool {
        if self.view != prepare.view {
            return false;
        }
        if self.seq_num != prepare.seq_num {
            return false;
        }
        if self.client_request_digest != prepare.client_request_digest {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckPoint {
    pub id: NodeId,
    pub committed_seq_num: usize,
    pub state_digest: Vec<u8>,
    pub state: HashMap<Key, Value>,
    pub checkpoint_commits: Vec<(Commit, ClientRequest)>,
    pub signature: Vec<u8>,
}

impl CheckPoint {
    pub fn new_with_signature(
        key_pair_bytes: Vec<u8>,
        id: usize,
        committed_seq_num: usize,
        state_digest: Vec<u8>,
        state: HashMap<Key, Value>,
        checkpoint_commits: Vec<(Commit, ClientRequest)>,
    ) -> Self {
        //todo make a signature
        let mut signature = Vec::new();

        Self {
            id,
            committed_seq_num,
            state_digest,
            state,
            checkpoint_commits,
            signature,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewChange {
    pub id: NodeId,
    pub new_view: usize,
    pub last_stable_seq_num: usize,
    pub checkpoint_proof: Vec<CheckPoint>,
    pub subsequent_pre_prepares: HashMap<usize, PrePrepare>,
    pub subsequent_prepares: HashMap<usize, Prepare>,
    pub signature: Vec<u8>,
}

impl ViewChange {
    
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ClientRequest {
    pub respond_addr: SocketAddr,
    pub time_stamp: usize,
    pub key: Key,
    pub value: Option<Value>,
}

impl ClientRequest {
    /// Hash of a Client Requyest used for a compressed version
    /// of the request in future messages
    pub fn digest(&self) -> Vec<u8> {
        let mut hasher = Sha512::new();
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

// Messages sent back to the client in response to requests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ClientResponse {
    pub id: NodeId,
    pub time_stamp: usize,
    pub key: Key,
    pub value: Option<Value>,
    pub success: bool,
    pub signature: Vec<u8>,
}

impl ClientResponse {
    pub fn new_with_signature(
        id: NodeId,
        time_stamp: usize,
        key: Key,
        value: Option<Value>,
        success: bool,
    ) -> ClientResponse {
        let mut signature = Vec::<u8>::new();
        ClientResponse {
            id,
            time_stamp,
            key,
            value,
            success,
            signature,
        }
    }
}

// Commands to Node

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeCommand {
    SendMessageCommand(SendMessage),
    BroadCastMessageCommand(BroadCastMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessage {
    pub destination: SocketAddr,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadCastMessage {
    pub message: Message,
}

// Commands to Consensus Engine

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusCommand {
    ProcessMessage(Message),
    MisdirectedClientRequest(ClientRequest),
    InitPrePrepare(ClientRequest),
    AcceptPrePrepare(PrePrepare),
    RebroadcastPrePrepare((usize, usize)),
    AcceptPrepare(Prepare),
    EnterCommit(Prepare),
    AcceptCommit(Commit),
    InitViewChange(ClientRequest),
    AcceptViewChange(ViewChange),
    ApplyCommit(Commit),
    AcceptCheckpoint(CheckPoint),
}
