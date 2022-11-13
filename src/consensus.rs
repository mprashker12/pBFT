use crate::config::Config;
use crate::messages::{
    BroadCastMessage, ClientRequest, ConsensusCommand, Message, NodeCommand, PrePrepare, Prepare,
    SendMessage,
};
use crate::NodeId;

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

// Note that all communication between the Node and the Consensus engine takes place
// by the outer consensus struct

pub struct Consensus {
    /// Id of the current node
    pub id: NodeId,
    /// Configuration of the cluster this node is in
    pub config: Config,
    /// Receiver of Consensus Commands
    pub rx_consensus: Receiver<ConsensusCommand>,
    /// Sends Commands to Node
    pub tx_node: Sender<NodeCommand>,
    /// Inner part of Consensus moving between tasks
    pub inner: InnerConsensus,
}

#[derive(Clone)]
pub struct InnerConsensus {
    /// Id of the current node
    pub id: NodeId,
    /// Configuration of the cluster this node is in
    pub config: Config,
    /// Send Consensus Commands back to the outer consensus engine
    pub tx_consensus: Sender<ConsensusCommand>,
    ///
    pub requests_seen: Arc<Mutex<HashMap<(usize, usize), ClientRequest>>>,

    pub prepare_votes: Arc<Mutex<HashMap<(usize, usize), HashSet<NodeId>>>>,

    pub outstanding_requests: Arc<Mutex<HashSet<ClientRequest>>>,
    /// Current state of conensus
    pub state: Arc<Mutex<State>>,
}

#[derive(Default)]
pub struct State {
    pub in_view_change: bool,
    pub view: usize,
    pub seq_num: usize,
    pub log: VecDeque<Message>,
}

impl Consensus {
    pub fn new(
        id: NodeId,
        config: Config,
        rx_consensus: Receiver<ConsensusCommand>,
        tx_consensus: Sender<ConsensusCommand>,
        tx_node: Sender<NodeCommand>,
    ) -> Self {
        let inner_consensus = InnerConsensus {
            id,
            config: config.clone(),
            tx_consensus,
            requests_seen: Arc::new(Mutex::new(HashMap::new())),
            prepare_votes: Arc::new(Mutex::new(HashMap::new())),
            outstanding_requests: Arc::new(Mutex::new(HashSet::new())),
            state: Arc::new(Mutex::new(State::default())),
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
                            // If we get a client request but are not the leader
                            // we forward the request to the leader. We started a timer
                            // which, if it expires and the request is still outstanding,
                            // will initiate the view change protocol

                            let leader = self.inner.current_leader().await;
                            let leader_addr = self.config.peer_addrs.get(&leader).unwrap();
                            let _ = self.tx_node.send(NodeCommand::SendMessageCommand(SendMessage {
                                destination: *leader_addr,
                                message: Message::ClientRequestMessage(request),
                            })).await;
                        }

                        ConsensusCommand::InitPrePrepare(request) => {
                            // Here we are primary and received a client request which we deemed valid
                            // so we broadcast a Pre_prepare Message to the network

                            let state = self.inner.state.lock().await;
                            let pre_prepare = PrePrepare {
                                id: self.id,
                                view: state.view,
                                seq_num: state.seq_num,
                                digest: 0,
                                signature: 0,
                                client_request: request,
                            };
                            let pre_prepare_message = Message::PrePrepareMessage(pre_prepare.clone());

                            let _ = self.tx_node.send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                                message: pre_prepare_message.clone()
                            })).await;

                        }

                        ConsensusCommand::AcceptPrePrepare(pre_prepare) => {
                            // We received a PrePrepare message from the network, and we see no violations
                            // So we will broadcast a corresponding prepare message and begin to count votes
                            let mut state = self.inner.state.lock().await;
                            let mut requests_seen = self.inner.requests_seen.lock().await;

                            requests_seen.insert((state.view, state.seq_num), pre_prepare.client_request.clone());

                            let prepare = Prepare {
                                id: self.id,
                                view: state.view,
                                seq_num: state.seq_num,
                                digest: 0,
                                signature: 0,
                            };

                            let prepare_message = Message::PrepareMessage(prepare.clone());
                            let _ = self.tx_node.send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                                message: prepare_message.clone(),
                            })).await;


                            state.log.push_back(Message::PrePrepareMessage(pre_prepare));
                            state.log.push_back(prepare_message);

                        }

                        ConsensusCommand::AcceptPrepare(prepare) => {
                            // We saw a prepare message from the network that we deemed was valid
                            // So we increment the vote count, and if we have enough prepare votes
                            // Then we move to the commit phases
                            let mut prepare_votes = self.inner.prepare_votes.lock().await;

                            if let Some(curr_vote_set) = prepare_votes.get_mut(&(prepare.view, prepare.seq_num)) {
                                curr_vote_set.insert(prepare.id);
                                if curr_vote_set.len() > self.config.num_faulty {
                                    // at this point, we have enough prepare votes to move into the commit phase. 
                                    let _ = self.inner.tx_consensus.send(ConsensusCommand::EnterCommit(prepare)).await;
                                }
                            } else {
                                let mut new_vote_set = HashSet::new();
                                new_vote_set.insert(prepare.id);
                                prepare_votes.insert((prepare.view, prepare.seq_num), new_vote_set);
                            }
                        }

                        ConsensusCommand::EnterCommit(prepare) => {
                            println!("BEGINNING COMMIT PHASE")
                        }
                    }
                }
            }
        }
    }
}

impl InnerConsensus {
    pub async fn process_message(&mut self, message: &Message) {
        // Note that we should not grab any locks here
        match message.clone() {
            Message::PrePrepareMessage(pre_prepare) => {
                if self.should_accept_pre_prepare(&pre_prepare).await {
                    let _ = self
                        .tx_consensus
                        .send(ConsensusCommand::AcceptPrePrepare(pre_prepare))
                        .await;
                }
            }
            Message::PrepareMessage(prepare) => {
                if self.should_accept_prepare(&prepare).await {
                    let _ = self
                        .tx_consensus
                        .send(ConsensusCommand::AcceptPrepare(prepare))
                        .await;
                }
            }
            Message::ClientRequestMessage(client_request) => {
                self.process_client_request(&client_request).await;
            }
        }
    }

    async fn current_leader(&self) -> NodeId {
        let state = self.state.lock().await;
        state.view % self.config.num_nodes
    }

    async fn add_to_log(&mut self, message: &Message) {
        let mut state = self.state.lock().await;
        state.log.push_back(message.clone());
    }

    async fn add_outstanding_request(&self, request: &ClientRequest) {
        let mut outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.insert(request.clone());
    }

    async fn remove_outstanding_request(&self, request: &ClientRequest) {
        let mut outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.remove(request);
    }

    async fn request_is_outstanding(&self, request: &ClientRequest) -> bool {
        let outstanding_requests = self.outstanding_requests.lock().await;
        outstanding_requests.contains(request)
    }

    async fn in_view_change(&self) -> bool {
        let state = self.state.lock().await;
        state.in_view_change
    }

    async fn increment_seq_num(&self) -> usize {
        let mut state = self.state.lock().await;
        let ret = state.seq_num;
        state.seq_num += 1;
        ret
    }

    async fn should_accept_pre_prepare(&self, message: &PrePrepare) -> bool {
        let state = self.state.lock().await;
        if state.view != message.view {
            return false;
        }
        // check if we have already seen a sequence number for this view
        true
    }

    async fn should_accept_prepare(&self, message: &Prepare) -> bool {
        let state = self.state.lock().await;
        if state.view != message.view {
            return false;
        }

        true
    }

    async fn process_client_request(&mut self, request: &ClientRequest) {
        if self.in_view_change().await {
            // if we are in the view change state
            // then we do not process any client requests
            return;
        }

        if self.id != self.current_leader().await {
            self.add_outstanding_request(request).await;
            let _ = self
                .tx_consensus
                .send(ConsensusCommand::MisdirectedClientRequest(request.clone()))
                .await;
            sleep(std::time::Duration::from_secs(7)).await;
            if self.request_is_outstanding(request).await {
                // add this point, we have hit the timeout for a request to be outstanding
                // so we need to trigger a view change
                self.init_view_change().await;
            }
            return;
        }
        // at this point we are the leader and we have accepted a client request
        // which we may begin to process
        let _ = self
            .tx_consensus
            .send(ConsensusCommand::InitPrePrepare(request.clone()))
            .await;
    }

    async fn init_view_change(&mut self) {
        println!("Triggering View Change...");
        let mut state = self.state.lock().await;
        state.in_view_change = true;
    }
}
