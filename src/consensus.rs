use crate::NodeId;
use crate::config::Config;
use crate::messages::{Message, PrePrepare, Prepare, ClientRequest, ConsensusCommand, NodeCommand, SendMessage};

use tokio::sync::Mutex;
use tokio::sync::mpsc::{Sender, Receiver};
use tokio::time::{sleep, Duration};

use std::collections::{VecDeque, HashSet};
use std::sync::Arc;

// Note that all communication between the Node and the Consensus engine takes place
// by the outer consensus struct

pub struct Consensus {
    /// Id of the current node
    pub id: NodeId,
    /// Configuration of the cluster this node is in
    pub config: Config,
    /// Receiver of Consensus Commands
    pub rx_consensus : Receiver<ConsensusCommand>,
    /// Sends Commands to Node
    pub tx_node : Sender<NodeCommand>,
    /// Inner part of Consensus moving between tasks
    pub inner: InnerConsensus,

}

#[derive(Clone)]
pub struct InnerConsensus {
    /// Id of the current node
    pub id : NodeId,
    /// Configuration of the cluster this node is in
    pub config: Config,
    /// Send Consensus Commands back to the outer consensus engine
    pub tx_consensus : Sender<ConsensusCommand>,

    pub outstanding_requests: Arc<Mutex<HashSet<ClientRequest>>>,
    /// Current state of conensus
    pub state : Arc<Mutex<State>>,
}

#[derive(Default)]
pub struct State {
    pub in_view_change: bool,
    pub view: usize,
    pub seq_num: usize,
    pub log: VecDeque<Message>,
}

impl Consensus {
    pub fn new(id : NodeId, config: Config, rx_consensus : Receiver<ConsensusCommand>, tx_consensus : Sender<ConsensusCommand>, tx_node : Sender<NodeCommand>) -> Self {
        
        let inner_consensus = InnerConsensus {
            id,
            config: config.clone(),
            tx_consensus,
            outstanding_requests: Arc::new(Mutex::new(HashSet::new())),
            state : Arc::new(Mutex::new(State::default())),
        };
        
        Self {
            id,
            config,
            rx_consensus,
            tx_node,
            inner: inner_consensus,
        }
    }

    pub async fn spawn(&mut self) {
        loop {
            tokio::select! {
                //main future listening for incoming commands
                res = self.rx_consensus.recv() => {
                    let cmd = res.unwrap();
                    println!("Consensus Engine Received Command {:?}", cmd);
                    match cmd {
                        ConsensusCommand::ProcessMessage(message) => {
                            let mut inner = self.inner.clone();
                            tokio::spawn(async move {
                                inner.process_message(&message).await;
                            });
                        }

                        ConsensusCommand::MisdirectedClientRequest(request) => {
                            let leader = self.inner.current_leader().await;
                            let leader_addr = self.config.peer_addrs.get(&leader).unwrap();
                            let _ = self.tx_node.send(NodeCommand::SendMessageCommand(SendMessage {
                                destination: *leader_addr,
                                message: Message::ClientRequestMessage(request),
                            })).await;
                        }
                    }
                }
            }
        }
    }
}

impl InnerConsensus {

    pub async fn process_message(&mut self, message: &Message) {

        match message.clone() {
            Message::PrePrepareMessage(pre_prepare) => {}
            Message::PrepareMessage(prepare) => {}
            Message::ClientRequestMessage(client_request) => {
                self.process_client_request(&client_request).await;
            }
        }

    }

    pub async fn current_leader(&self) -> NodeId {
        let state = self.state.lock().await;
        state.view % self.config.num_nodes
    }

    pub async fn add_to_log(&mut self, message: &Message) {
        let mut state = self.state.lock().await;
        state.log.push_back(message.clone());
    }

    pub async fn add_outstanding_request(&self, request: &ClientRequest) {
        let mut outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.insert(request.clone());
    }

    pub async fn remove_outstanding_request(&self, request: &ClientRequest) {
        let mut outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.remove(request);
    }

    pub async fn request_is_outstanding(&self, request: &ClientRequest) -> bool {
        let outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.contains(request)
    }

    pub async fn should_accept_pre_prepare(&self, message: &PrePrepare) -> bool {
        let state = self.state.lock().await;
        if state.view != message.view {return false;}
        // check if we have already seen a sequence number for this view
        true
    }

    pub async fn should_accept_prepare(&self, message: &Prepare) -> bool {
        let state = self.state.lock().await;
        if state.view != message.view {return false;}

        true
    }

    pub async fn process_client_request(&mut self, request: &ClientRequest) {
        if self.id != self.current_leader().await {
            self.add_outstanding_request(request).await;
            self.tx_consensus.send(ConsensusCommand::MisdirectedClientRequest(request.clone())).await;
            sleep(std::time::Duration::from_secs(7)).await;
            if self.request_is_outstanding(request).await {
                // add this point, we have hit the timeout for a request to be outstanding
                // so we need to trigger a view change
                println!("Triggering view change");
            }
        }
    }
}
