use crate::messages::{CheckPoint, ClientRequest, Commit, PrePrepare, Prepare};


use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct MessageBank {
    /// (view, PrePrepare) requests which were sent either as the leader
    /// or were rebroadcast to the leader if this node is a replica
    /// This is used to make sure we do not broadcast the same request multiple times to the network
    pub sent_requests: HashSet<(usize, ClientRequest)>,
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
        let mut old_view_seq_num_pairs = vec![];
        for ((view, seq_num), _) in self.accepted_pre_prepare_requests.iter()  {
            if *seq_num < upper_seq_num {
                old_view_seq_num_pairs.push((*view, *seq_num));
            }
        }

        for (view, seq_num) in old_view_seq_num_pairs.iter() {
            self.accepted_pre_prepare_requests.remove(&(*view, *seq_num));
        }

    }
}
