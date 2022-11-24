use crate::config::Config;
use crate::messages::{ClientRequest, ConsensusCommand, PrePrepare};
use crate::NodeId;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::Sender;
use tokio::time::sleep;

#[derive(Clone)]
pub struct ViewChanger {
    /// Id of the current node
    pub id: NodeId,
    /// Configuration of the cluster this node is in
    pub config: Config,
    /// Send Consensus Commands back to the outer consensus engine
    pub tx_consensus: Sender<ConsensusCommand>,
    /// These are added when we either get a misdirected client request
    /// or we accept a pre-prepare message
    /// Used to initiate view changes
    pub wait_set: Arc<Mutex<HashSet<ClientRequest>>>,
    /// These are pre-prepares sent by the leader which we have not applied yet
    /// If a certain amount of time expires and we have not yet applied it
    /// we re-broadcast the pre-prepare to the other peers
    /// Pre-prepares are indexed by (view, seq_num)
    pub sent_pre_prepares: Arc<Mutex<HashSet<(usize, usize)>>>,
}

impl ViewChanger {
    pub fn add_to_sent_pre_prepares(&mut self, view_seq_num_pair: &(usize, usize)) -> bool {
        let mut sent_pre_prepares = self.sent_pre_prepares.lock().unwrap();
        sent_pre_prepares.insert(*view_seq_num_pair)
    }

    pub fn remove_from_sent_pre_prepares(&mut self, view_seq_num_pair: &(usize, usize)) {
        let mut sent_pre_prepares = self.sent_pre_prepares.lock().unwrap();
        sent_pre_prepares.remove(view_seq_num_pair);
    }

    pub fn is_in_sent_pre_prepares(&self, view_seq_num_pair: &(usize, usize)) -> bool {
        let sent_pre_prepares = self.sent_pre_prepares.lock().unwrap();
        sent_pre_prepares.contains(view_seq_num_pair)
    }

    pub async fn wait_for_sent_pre_prepares(&self, view_seq_num_pair: &(usize, usize)) {
        sleep(std::time::Duration::from_secs(2)).await;
        if self.is_in_sent_pre_prepares(&view_seq_num_pair.clone()) {
            let _ = self
                .tx_consensus
                .send(ConsensusCommand::RebroadcastPrePrepare(*view_seq_num_pair))
                .await;
        }
    }

    pub fn add_to_wait_set(&mut self, request: &ClientRequest) -> bool {
        let mut outstanding_requests = self.wait_set.lock().unwrap();
        outstanding_requests.insert(request.clone())
    }

    pub fn remove_from_wait_set(&mut self, request: &ClientRequest) {
        let mut outstanding_requests = self.wait_set.lock().unwrap();
        outstanding_requests.remove(request);
    }

    pub fn is_in_wait_set(&self, request: &ClientRequest) -> bool {
        let outstanding_requests = self.wait_set.lock().unwrap();
        outstanding_requests.contains(request)
    }

    pub async fn wait_for(&self, request: &ClientRequest) {
        sleep(self.config.request_timeout).await;
        if self.is_in_wait_set(&request.clone()) {
            let _ = self
                .tx_consensus
                .send(ConsensusCommand::InitViewChange(request.clone()))
                .await;
        }
    }

    pub fn reset(&mut self) {
        let mut wait_set = self.wait_set.lock().unwrap();
        wait_set.clear();

        let mut sent_pre_prepares = self.sent_pre_prepares.lock().unwrap();
        sent_pre_prepares.clear();
    }
}
