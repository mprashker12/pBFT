use crate::messages::{CheckPoint, ClientRequest, Commit, Message, PrePrepare, Prepare};
use crate::NodeId;

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
pub struct MessageBank {
    /// The log of accepted messages
    pub log: VecDeque<Message>,
    /// PrePrepare requests which were sent as the leader
    /// This is used to make sure we do not broadcast the same request multiple times to the network
    pub sent_requests: HashSet<ClientRequest>,
    /// Pre-prepare messages by (view, seq_num) that
    /// we have accepted but have not applied yet
    pub accepted_pre_prepare_requests: HashMap<(usize, usize), PrePrepare>,
    /// Valid prepares that we received that we did not accept
    /// (These have been buffered because we may not have received the associated pre-prepare)
    pub outstanding_prepares: HashSet<Prepare>,
    /// Valid commits that we received that we did not accept
    /// (These have been buffered because we may not have received the associated prepare)
    pub outstanding_commits: HashSet<Commit>,
    /// Commits we accepted but did not apply the associated request yet
    pub accepted_commits_not_applied: HashMap<usize, Commit>,
    /// Maps a sequence number to the commit applied at a given sequence number
    /// together with the associated client request
    pub applied_commits: HashMap<usize, (Commit, ClientRequest)>,
    /// Maps a (seq_num, state_digest) pair to checkpoints we saw for that pair
    pub checkpoint_messages: HashMap<(usize, Vec<u8>), CheckPoint>,
}

impl MessageBank {
    /// Removes all state pertaining to messages with
    /// with sequence number < upper_seq_num
    pub fn garbage_collect(&mut self, upper_seq_num: usize) {
        
    }
}
