use crate::{NodeId};
use crate::messages::{Message, ClientRequest, Prepare, Commit};

use std::collections::{VecDeque, HashMap, HashSet};

#[derive(Default)]
pub struct MessageBank {
    /// The log of accepted messages
    pub log: VecDeque<Message>,
    pub seen_requests: HashMap<(usize, usize), ClientRequest>,
    pub outstanding_prepares: HashSet<Prepare>,
    pub outstanding_commits: HashSet<Commit>,
    pub prepare_votes: HashMap<(usize, usize), HashSet<NodeId>>,
    pub commit_votes: HashMap<(usize, usize), HashSet<NodeId>>,
}