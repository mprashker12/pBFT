use crate::config::Config;
use crate::messages::{ClientRequest, ConsensusCommand};
use crate::NodeId;

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
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
    pub outstanding_requests: Arc<Mutex<HashSet<ClientRequest>>>,
}

impl ViewChanger {
    pub async fn add_outstanding_request(&mut self, request: &ClientRequest) -> bool {
        let mut outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.insert(request.clone())
    }

    pub async fn remove_outstanding_request(&mut self, request: &ClientRequest) {
        let mut outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.remove(request);
    }

    pub async fn request_is_outstanding(&self, request: &ClientRequest) -> bool {
        let outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.contains(request)
    }

    pub async fn wait_for_outstanding(&self, request: &ClientRequest) {
        sleep(std::time::Duration::from_secs(5)).await;
        if self.request_is_outstanding(&request.clone()).await {
            let _ = self
                .tx_consensus
                .send(ConsensusCommand::InitViewChange(request.clone()))
                .await;
        }
    }
}
