use crate::config::Config;
use crate::messages::{
    BroadCastMessage, ClientRequest, Commit, ConsensusCommand, Message, NodeCommand, PrePrepare,
    Prepare, SendMessage,
};
use crate::state::State;
use crate::{Key, NodeId, Value};
use crate::view_changer::ViewChanger;

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Mutex;


use std::collections::{HashMap, HashSet};
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

    pub tx_consensus: Sender<ConsensusCommand>,
    /// Maps (view, seq_num) to client request seen for that pair
    /// A request is inserted once we accept a pre-prepare message from the network for this request
    pub requests_seen: Mutex<HashMap<(usize, usize), ClientRequest>>,

    pub prepare_votes: Mutex<HashMap<(usize, usize), HashSet<NodeId>>>,

    pub commit_votes: Mutex<HashMap<(usize, usize), HashSet<NodeId>>>,
    /// Current state of conensus
    pub state: Mutex<State>,
    /// Inner part of Consensus moving between tasks
    pub view_changer: ViewChanger,
}


impl Consensus {
    pub fn new(
        id: NodeId,
        config: Config,
        rx_consensus: Receiver<ConsensusCommand>,
        tx_consensus: Sender<ConsensusCommand>,
        tx_node: Sender<NodeCommand>,
    ) -> Self {
        let view_changer = ViewChanger {
            id,
            config: config.clone(),
            tx_consensus: tx_consensus.clone(),
            outstanding_requests: Arc::new(Mutex::new(HashSet::new())),
        };

        Self {
            id,
            config,
            rx_consensus,
            tx_node,
            tx_consensus,
            requests_seen: Mutex::new(HashMap::new()),
            prepare_votes: Mutex::new(HashMap::new()),
            commit_votes: Mutex::new(HashMap::new()),
            state: Mutex::new(State::default()),
            view_changer,
        }
    }

    pub async fn spawn(&mut self) {
        loop {
            tokio::select! {
                // main future listening for incoming commands
                res = self.rx_consensus.recv() => {
                    let mut state = self.state.lock().await;
                    let cmd = res.unwrap();
                    println!("Consensus Engine Received Command {:?}", cmd);
                    match cmd {
                        ConsensusCommand::ProcessMessage(message) => {
                            match message.clone() {
                                Message::PrePrepareMessage(pre_prepare) => {
                                    if self.should_accept_pre_prepare(&state, &pre_prepare) {
                                        let _ = self
                                            .tx_consensus
                                            .send(ConsensusCommand::AcceptPrePrepare(pre_prepare))
                                            .await;
                                    }
                                }
                                Message::PrepareMessage(prepare) => {
                                    if self.should_accept_prepare(&state, &prepare).await {
                                        let _ = self
                                            .tx_consensus
                                            .send(ConsensusCommand::AcceptPrepare(prepare))
                                            .await;
                                    }
                                }
                                Message::CommitMessage(commit) => {
                                    if self.should_accept_commit(&state, &commit).await {
                                        let _ = self
                                            .tx_consensus
                                            .send(ConsensusCommand::AcceptCommit(commit))
                                            .await;
                                    }
                                }
                                Message::ClientRequestMessage(client_request) => {
                                    if self.should_process_client_request(&state, &client_request).await {
                                        let _ = self
                                            .tx_consensus.
                                            send(ConsensusCommand::ProcessClientRequest(client_request)).
                                            await;
                                    }
                                }
                                Message::ClientResponseMessage(_) => {
                                    // we should never receive a client response message
                                }
                            }
                        }

                        ConsensusCommand::MisdirectedClientRequest(request) => {
                            // If we get a client request but are not the leader
                            // we forward the request to the leader. We started a timer
                            // which, if it expires and the request is still outstanding,
                            // will initiate the view change protocol

                            let leader = self.current_leader(&state);
                            let leader_addr = self.config.peer_addrs.get(&leader).unwrap();
                            let _ = self.tx_node.send(NodeCommand::SendMessageCommand(SendMessage {
                                destination: *leader_addr,
                                message: Message::ClientRequestMessage(request.clone()),
                            })).await;


                            let newly_added = self.view_changer.add_outstanding_request(&request).await;
                            if newly_added {
                                let view_changer = self.view_changer.clone();
                                tokio::spawn(async move {
                                    view_changer.wait_for_outstanding(&request.clone()).await;
                                });
                            }
                        }

                        ConsensusCommand::ProcessClientRequest(request) => {


                            if self.id != self.current_leader(&state){
                                let _ = self.view_changer
                                    .tx_consensus
                                    .send(ConsensusCommand::MisdirectedClientRequest(request.clone()))
                                    .await;
                            } else {
                            // at this point we are the leader and we have accepted a client request
                            // which we may begin to process
                            let _ = self.view_changer
                                .tx_consensus
                                .send(ConsensusCommand::InitPrePrepare(request.clone()))
                                .await;
                            }
                        }

                        ConsensusCommand::InitPrePrepare(request) => {
                            // Here we are primary and received a client request which we deemed valid
                            // so we broadcast a Pre_prepare Message to the network


                            let pre_prepare = PrePrepare {
                                id: self.id,
                                view: state.view,
                                seq_num: state.seq_num,
                                digest: request.clone().hash(),
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
                            let mut requests_seen = self.requests_seen.lock().await;
                            requests_seen.insert((state.view, state.seq_num), pre_prepare.client_request.clone());

                            let prepare = Prepare {
                                id: self.id,
                                view: state.view,
                                seq_num: state.seq_num,
                                digest: pre_prepare.clone().digest,
                                signature: 0,
                            };

                            let prepare_message = Message::PrepareMessage(prepare.clone());
                            let _ = self.tx_node.send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                                message: prepare_message.clone(),
                            })).await;


                            state.log.push_back(Message::PrePrepareMessage(pre_prepare.clone()));
                            state.log.push_back(prepare_message);


                            // at this point, we need to trigger a timer, and if the timer expires
                            // and the request is still outstanding, then we need to trigger a view change
                            // as this is evidence that the system has stopped making progress
                            let newly_added = self.view_changer.add_outstanding_request(&pre_prepare.client_request).await;
                            if newly_added {
                                println!("Launching wait");
                                let view_changer = self.view_changer.clone();
                                tokio::spawn(async move {
                                    view_changer.wait_for_outstanding(&pre_prepare.client_request).await;
                                });
                            }
                        }

                        ConsensusCommand::AcceptPrepare(prepare) => {
                            // We saw a prepare message from the network that we deemed was valid
                            // So we increment the vote count, and if we have enough prepare votes
                            // Then we move to the commit phases

                            let mut prepare_votes = self.prepare_votes.lock().await;


                            state.log.push_back(Message::PrepareMessage(prepare.clone()));

                            if let Some(curr_vote_set) = prepare_votes.get_mut(&(prepare.view, prepare.seq_num)) {
                                curr_vote_set.insert(prepare.id);
                                if curr_vote_set.len() > 2*self.config.num_faulty {
                                    // at this point, we have enough prepare votes to move into the commit phase.
                                    let _ = self.view_changer.tx_consensus.send(ConsensusCommand::EnterCommit(prepare)).await;
                                }
                            } else {
                                // first time we got a prepare message for this view and sequence number
                                let mut new_vote_set = HashSet::new();
                                new_vote_set.insert(prepare.id);
                                prepare_votes.insert((prepare.view, prepare.seq_num), new_vote_set);
                            }
                        }

                        ConsensusCommand::EnterCommit(prepare) => {


                            println!("BEGINNING COMMIT PHASE");
                            let commit = Commit {
                                id: self.id,
                                view: state.view,
                                seq_num: state.seq_num,
                                digest: prepare.digest,
                                signature: 0,
                            };
                            let commit_message = Message::CommitMessage(commit);
                            let _ = self.tx_node.send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                                message: commit_message,
                            })).await;
                        }

                        ConsensusCommand::AcceptCommit(commit) => {
                            // We received a Commit Message for a request that we deemed valid
                            // so we increment the vote count


                            let mut commit_votes = self.commit_votes.lock().await;
                            println!("Got commit from {}", commit.id);

                            state.log.push_back(Message::CommitMessage(commit.clone()));

                            if let Some(curr_vote_set) = commit_votes.get_mut(&(commit.view, commit.seq_num)) {
                                curr_vote_set.insert(commit.id);
                                if curr_vote_set.len() > 2*self.config.num_faulty {
                                    // At this point, we have enough commit votes to commit the message
                                    let _ = self.view_changer.tx_consensus.send(ConsensusCommand::ApplyClientRequest(commit)).await;
                                }
                            } else {
                                // first time we got a prepare message for this view and sequence number
                                let mut new_vote_set = HashSet::new();
                                new_vote_set.insert(commit.id);
                                commit_votes.insert((commit.view, commit.seq_num), new_vote_set);
                            }
                        }

                        ConsensusCommand::InitViewChange(request) => {

                            if state.in_view_change || self.current_leader(&state) == self.id {
                                // we are already in a view change state or we are currently the leader
                                return;
                            }
                            println!("Initializing view change...");
                            state.in_view_change = true;
                        }

                        ConsensusCommand::ApplyClientRequest(commit) => {
                            // we now have permission to apply the client request

                            let requests_seen = self.requests_seen.lock().await;
                            //remove the request from the outstanding requests so that we can trigger the view change
                            let client_request = requests_seen.get(&(commit.view, commit.seq_num)).unwrap();
                            self.view_changer.remove_outstanding_request(client_request).await;

                            println!("Applying client request");

                            if client_request.value.is_some() {
                                // request is a set request
                                state.store.insert(client_request.clone().key, client_request.clone().value.unwrap());
                            } {
                                //request is a get request
                            }

                            state.seq_num += 1;

                        }
                    }
                }
            }
        }
    }

    // todo: move these functions into the state struct
    fn current_leader(&self, state: &State) -> NodeId {
        state.view % self.config.num_nodes
    }

    fn should_accept_pre_prepare(&self, state: &State, message: &PrePrepare) -> bool {
        if state.view != message.view {
            return false;
        }
        // verify that the digest of the message is equal to the hash of the client_request
        // check if we have already seen a sequence number for this view
        // have we accepted a pre-prepare message in this view with same sequence number and different digest
        true
    }

    async fn should_accept_prepare(&self, state: &State, message: &Prepare) -> bool {
        let requests_seen = self.requests_seen.lock().await;

        if state.view != message.view {
            return false;
        }

        // make sure we already saw a request with given view and sequence number,
        // and make sure that the digests are correct.
        if let Some(e_pre_prepare) = requests_seen.get(&(message.view, message.seq_num)) {
            if message.digest != *e_pre_prepare.hash() {
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

    async fn should_accept_commit(&self, state: &State, messsage: &Commit) -> bool {
        true
    }

    async fn should_process_client_request(&self, state: &State, request: &ClientRequest) -> bool {
        if state.in_view_change {
            // if we are in the view change state
            // then we do not process any client requests
            return false;
        }
        true
    }
}


