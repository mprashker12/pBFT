use crate::config::Config;
use crate::message_bank::MessageBank;
use crate::messages::{
    CheckPoint, ClientRequest, ClientResponse, Commit, Message, PrePrepare, Prepare, ViewChange, NewView,
};
use crate::node::Node;
use crate::{Key, NodeId, Value};

use std::collections::{HashMap, HashSet, VecDeque};

use ed25519_dalek::{Digest, Sha512};

#[derive(Default)]
pub struct State {
    /// Configuration data of the cluster this node is in.
    pub config: Config,
    /// Id of the node which maintains this state
    pub id: NodeId,
    /// Has this node issued a view-change message which has not been resolved
    pub in_view_change: bool,
    /// Current view we are in
    pub view: usize,
    /// Used by the leader to determine the next sequence number to assign a client request
    pub seq_num: usize,
    /// Sequence number of last stable checkpoint
    pub last_stable_seq_num: usize,
    /// Sequence number of last request which was applied to the state
    pub last_seq_num_committed: usize,
    /// Maps (view, seq_num) to Ids of nodes who we
    /// have accepted prepare messages from for the associated transaction
    pub prepare_votes: HashMap<(usize, usize), HashMap<NodeId, Prepare>>,
    /// Maps (view, seq_num) to Ids of nodes who we
    /// have accepted prepare messages from for the associated transaction
    pub commit_votes: HashMap<(usize, usize), HashSet<NodeId>>,
    /// Maps (seq_num, digest) pair to the Ids of the nodes
    /// who we have received corresponding checkpoints from
    pub checkpoint_votes: HashMap<(usize, Vec<u8>), HashSet<NodeId>>,
    /// Maps a NodeId to the most recent checkpoint we have seen from that node
    /// since the last stable checkpoint
    /// Note that this is reset every time we hit a new checkpoint
    pub checkpoints_current_round: HashMap<usize, CheckPoint>,
    /// This consists of 2f + 1 checkpoint messages which were used to
    /// move to the current stable checkpoint
    /// This is used for subsequent view change messages
    pub last_checkpoint_proof: Vec<CheckPoint>,
    /// Maps node Ids to view change messages we have received for the subsequent view
    /// Note that we only ever maintain <= 1 view_change message from any given node
    pub view_change_votes: HashMap<NodeId, ViewChange>,
    /// Structure storing all messages, including log
    pub message_bank: MessageBank,
    /// Key-Value store which the system actually maintains
    pub store: HashMap<Key, Value>,
}
impl State {
    pub fn current_leader(&self) -> NodeId {
        self.get_leader_for_view(self.view)
    }

    pub fn get_leader_for_view(&self, view: usize) -> NodeId {
        view % self.config.num_nodes
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
        if let Some(e_pre_prepare) = self
            .message_bank
            .accepted_pre_prepare_requests
            .get(&(pre_prepare.view, pre_prepare.seq_num))
        {
            // if we already saw a pre-prepare request for this (view, seq-num) pair,
            // then we will accept as along as the message digests are the same
            return e_pre_prepare.client_request_digest == pre_prepare.client_request_digest;
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

    pub fn should_accept_commit(&self, commit: &Commit) -> bool {
        if self.in_view_change {
            return false;
        }
        if self.view != commit.view {
            return false;
        }
        true
    }

    pub fn should_process_client_request(&self, _request: &ClientRequest) -> bool {
        // this will only be called by the master replica
        if self.in_view_change {
            return false;
        }
        // maintain timestamp of last request we applied to our state from the client
        // if the timestamp is too small, then we do not accept the message

        true
    }

    pub fn should_accept_checkpoint(&self, _checkpoint: &CheckPoint) -> bool {
        // note that we accept checkpoint messages as long as they have been properly signed,
        // which must be the case by the time the message gets to this consensus layer
        true
    }

    pub fn should_accept_view_change(&self, view_change: &ViewChange) -> bool {
        // make sure the view change is for the next view
        if view_change.new_view != self.view + 1 {
            return false;
        }
        // if we are not the leader for the next view, then we don't accept the view change
        if self.get_leader_for_view(view_change.new_view) != self.id {
            return false;
        }


        true
    }

    pub fn should_accept_new_view(&self, _new_view: &NewView) -> bool {
        // as long as the new view message is well formed, we should always accept it
        true
    }

    pub fn apply_commit(
        &mut self,
        request: &ClientRequest,
        commit: &Commit,
    ) -> (Option<Option<&Value>>, Vec<Commit>) {
        
        self.last_seq_num_committed = commit.seq_num;
        self.message_bank
            .accepted_commits_not_applied
            .remove(&(commit.seq_num));

        self.message_bank
            .applied_commits
            .insert(commit.seq_num, (commit.clone(), request.clone()));

        let commit_res = if request.value.is_some() {
            // request is a set request
            self.store
                .insert(request.clone().key, request.clone().value.unwrap());
            None
        } else {
            //request is a get request
            let ret = self.store.get(&request.key);
            Some(ret)
        };

        // determine if there are any outstanding commits which we can now apply
        let mut new_applies = Vec::<Commit>::new();
        let mut try_commit = commit.seq_num + 1;

        while let Some(commit) = self
            .message_bank
            .accepted_commits_not_applied
            .get(&try_commit)
        {
            new_applies.push(commit.clone());
            try_commit += 1;
        }

        (commit_res, new_applies)
    }

    pub fn update_checkpoint_meta(&mut self, seq_num: &usize, state_digest: &[u8]) {
        self.last_checkpoint_proof.clear();

        let curr_vote_set = self
            .checkpoint_votes
            .get(&(*seq_num, state_digest.to_vec()))
            .unwrap();
        for node_id in curr_vote_set.iter() {
            self.last_checkpoint_proof
                .push(self.checkpoints_current_round.get(node_id).unwrap().clone());
        }

        self.checkpoint_votes.clear();
        self.checkpoints_current_round.clear();
    }

    pub fn garbage_collect(&mut self) {
        self.message_bank.garbage_collect(self.last_stable_seq_num);

        //todo: remove all messages from prepare_votes and checkpoint votes that pertain to old messages
    }

    /// Sha512 hash of the state store
    pub fn digest(&self) -> Vec<u8> {
        let mut hasher = Sha512::new();

        let state_bytes = serde_json::to_string(&self.store)
            .unwrap()
            .as_bytes()
            .to_vec();

        hasher.update(state_bytes);
        hasher.finalize().as_slice().to_vec()
    }
}
