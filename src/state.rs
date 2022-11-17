use crate::config::Config;
use crate::message_bank::MessageBank;
use crate::messages::{ClientRequest, Commit, Message, PrePrepare, Prepare};
use crate::{Key, NodeId, Value};
use std::collections::{HashMap, HashSet, VecDeque};

use sha2::{Digest, Sha256};

#[derive(Default)]
pub struct State {
    pub config: Config,
    pub in_view_change: bool,
    pub view: usize,
    pub seq_num: usize,
    pub last_seq_num_committed: usize,
    /// Maps (view, seq_num) to Ids of nodes who we
    /// have accepted prepare messages from for the associated transaction
    pub prepare_votes: HashMap<(usize, usize), HashSet<NodeId>>,
    /// Maps (view, seq_num) to Ids of nodes who we
    /// have accepted prepare messages from for the associated transaction
    pub commit_votes: HashMap<(usize, usize), HashSet<NodeId>>,
    /// Structure storing all messages, including log
    pub message_bank: MessageBank,
    pub store: HashMap<Key, Value>,
}
impl State {
    // todo: move these functions into the state struct
    pub fn current_leader(&self) -> NodeId {
        self.view % self.config.num_nodes
    }

    pub fn should_accept_pre_prepare(&self, pre_prepare: &PrePrepare) -> bool {
        if self.in_view_change {
            return false;
        }
        if self.view != pre_prepare.view {
            return false;
        }
        if pre_prepare.client_request_digest != pre_prepare.client_request.digest() {
            return false;
        }
        if self
            .message_bank
            .accepted_pre_prepare_requests
            .contains_key(&(pre_prepare.view, pre_prepare.seq_num))
        {
            return false;
        }

        true
    }

    pub fn should_accept_prepare(&self, prepare: &Prepare) -> bool {
        if self.in_view_change {
            return false;
        }
        if self.view != prepare.view {
            return false;
        }

        // make sure we already saw a request with given view and sequence number,
        // and make sure that the digests are correct.
        if let Some(e_request) = self
            .message_bank
            .accepted_pre_prepare_requests
            .get(&(prepare.view, prepare.seq_num))
        {
            if prepare.client_request_digest != *e_request.client_request.digest() {
                return false;
            }
        } else {
            // we have not seen a pre_prepare message for any request
            // with this given (view, seq_num) pair, so we cannot accept a prepare
            // for this request

            return false;
        }
        true
    }

    pub fn should_accept_commit(&self, messsage: &Commit) -> bool {
        true
    }

    pub fn should_process_client_request(&self, request: &ClientRequest) -> bool {
        // this will only be called by the master replica
        if self.in_view_change {
            return false;
        }
        true
    }

    pub fn apply_commit(&mut self, request: &ClientRequest, commit: &Commit) {
        // todo - get the request from the commit view and seq num

        if request.value.is_some() {
            // request is a set request
            self.store
                .insert(request.clone().key, request.clone().value.unwrap());
        } else {

            //request is a get request
        }

        self.last_seq_num_committed = commit.seq_num;
    }

    /// Sha256 hash of the state store
    pub fn digest(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();

        let state_bytes = serde_json::to_string(&self.store)
            .unwrap()
            .as_bytes()
            .to_vec();

        hasher.update(state_bytes);
        hasher.finalize().as_slice().to_vec()
    }
}
