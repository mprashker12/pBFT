use crate::messages::{ClientRequest, Commit, Message, Prepare};
use crate::NodeId;

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
pub struct MessageBank {
    /// The log of accepted messages
    pub log: VecDeque<Message>,
    pub accepted_prepare_requests: HashMap<(usize, usize), ClientRequest>,
    /// Valid prepares that we received that we did not accept
    pub outstanding_prepares: HashSet<Prepare>,
    /// Valid commits that we received that we did not accept
    pub outstanding_commits: HashSet<Commit>,
}
