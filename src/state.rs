use crate::config::Config;
use crate::message_bank::MessageBank;
use crate::messages::{Message, PrePrepare, Prepare, Commit, ClientRequest};
use crate::{Key, Value, NodeId};
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub struct State {
    pub config: Config,
    pub in_view_change: bool,
    pub view: usize,
    pub seq_num: usize,
    pub last_seq_num_committed: usize,
    pub message_bank: MessageBank,
    pub store: HashMap<Key, Value>,
}
impl State {
    // todo: move these functions into the state struct
    pub fn current_leader(&self) -> NodeId {
        self.view % self.config.num_nodes
    }

    pub fn should_accept_pre_prepare(&self, message: &PrePrepare) -> bool {
        if self.view != message.view {
            return false;
        }
        // verify that the digest of the message is equal to the hash of the client_request
        // check if we have already seen a sequence number for this view
        // have we accepted a pre-prepare message in this view with same sequence number and different digest
        true
    }

    pub fn should_accept_prepare(&self, message: &Prepare) -> bool {
        if self.view != message.view {
            return false;
        }

        // make sure we already saw a request with given view and sequence number,
        // and make sure that the digests are correct.
        if let Some(e_request) = self.message_bank.seen_requests.get(&(message.view, message.seq_num)) {
            if message.digest != *e_request.hash() {
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
        if self.in_view_change {
            // if we are in the view change state
            // then we do not process any client requests
            return false;
        }
        true
    }

    pub fn apply_commit(&mut self, request: &ClientRequest, commit: &Commit) {

        
        if request.value.is_some() {
            // request is a set request
            self.store.insert(
                request.clone().key,
                request.clone().value.unwrap(),
            );
        } else {
        
            //request is a get request
        }

        self.last_seq_num_committed = commit.seq_num;
    }
}