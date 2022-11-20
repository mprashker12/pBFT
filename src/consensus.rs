use crate::config::Config;
use crate::messages::{
    BroadCastMessage, CheckPoint, ClientRequest, Commit, ConsensusCommand, Message, NodeCommand,
    PrePrepare, Prepare, SendMessage, ClientResponse
};
use crate::state::State;
use crate::view_changer::{self, ViewChanger};
use crate::NodeId;

use tokio::sync::mpsc::{Receiver, Sender};

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use log::{debug, error, info, log_enabled, Level};

// Note that all communication between the Node and the Consensus engine takes place
// by the outer consensus struct

pub struct Consensus {
    /// Id of the current node
    pub id: NodeId,
    /// Configuration of the cluster this node is in
    pub config: Config,
    /// Keypair of the node
    pub keypair_bytes: Vec<u8>,
    /// Receiver of Consensus Commands
    pub rx_consensus: Receiver<ConsensusCommand>,
    /// Sends Commands to Node
    pub tx_node: Sender<NodeCommand>,
    /// Sends Consensus Commands to itself
    pub tx_consensus: Sender<ConsensusCommand>,
    /// Current State of the Consensus
    pub state: State,
    /// Responsible for outstanding requests and changing views
    pub view_changer: ViewChanger,
}

impl Consensus {
    pub fn new(
        id: NodeId,
        config: Config,
        keypair_bytes: Vec<u8>,
        rx_consensus: Receiver<ConsensusCommand>,
        tx_consensus: Sender<ConsensusCommand>,
        tx_node: Sender<NodeCommand>,
    ) -> Self {
        let state = State {
            config: config.clone(),
            ..Default::default()
        };

        let view_changer = ViewChanger {
            id,
            config: config.clone(),
            tx_consensus: tx_consensus.clone(),
            wait_set: Arc::new(Mutex::new(HashSet::new())),
            sent_pre_prepares: Arc::new(Mutex::new(HashSet::new())),
        };

        Self {
            id,
            config,
            keypair_bytes,
            rx_consensus,
            tx_node,
            tx_consensus,
            state,
            view_changer,
        }
    }

    pub async fn spawn(&mut self) {
        loop {
            let res = self.rx_consensus.recv().await;
            let cmd = res.unwrap();
            //info!("Consensus Engine Received Command {:?}", cmd);
            match cmd {
                ConsensusCommand::ProcessMessage(message) => {
                    match message.clone() {
                        Message::IdentifierMessage(_) => {
                            unreachable!()
                        }

                        Message::PrePrepareMessage(pre_prepare) => {
                            info!("Saw preprepare from {}", pre_prepare.id);
                            if self.state.should_accept_pre_prepare(&pre_prepare) {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::AcceptPrePrepare(pre_prepare))
                                    .await;
                            }
                        }
                        Message::PrepareMessage(prepare) => {
                            info!("Saw prepare from {}", prepare.id);
                            if self.state.should_accept_prepare(&prepare) {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::AcceptPrepare(prepare))
                                    .await;
                            } else {
                                self.state
                                    .message_bank
                                    .outstanding_prepares
                                    .insert(prepare.clone());
                            }
                        }
                        Message::CommitMessage(commit) => {
                            info!("Saw commit from {}", commit.id);
                            if self.state.should_accept_commit(&commit) {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::AcceptCommit(commit))
                                    .await;
                            } else {
                                self.state
                                    .message_bank
                                    .outstanding_commits
                                    .insert(commit.clone());
                            }
                        }

                        Message::ViewChangeMessage(view_change) => {}
                        Message::CheckPointMessage(checkpoint) => {
                            info!("Saw checkpoint from {}", checkpoint.id);

                            if self.state.should_accept_checkpoint(&checkpoint) {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::AcceptCheckpoint(checkpoint))
                                    .await;
                            }
                        }

                        Message::ClientRequestMessage(client_request) => {
                            info!("Saw client request");
                            if self.state.should_process_client_request(&client_request) {
                                if self.id != self.state.current_leader() {
                                    let _ = self
                                        .tx_consensus
                                        .send(ConsensusCommand::MisdirectedClientRequest(
                                            client_request.clone(),
                                        ))
                                        .await;
                                } else {
                                    // at this point we are the leader and we have accepted a client request
                                    // which we may begin to process
                                    let _ = self
                                        .tx_consensus
                                        .send(ConsensusCommand::InitPrePrepare(
                                            client_request.clone(),
                                        ))
                                        .await;
                                }
                            }
                        }

                        Message::ClientResponseMessage(_) => {
                            // we should never receive a client response message
                            unreachable!()
                        }
                    }
                }

                ConsensusCommand::MisdirectedClientRequest(request) => {
                    // If we get a client request but are not the leader
                    // we forward the request to the leader. We started a timer
                    // which, if it expires and the request is still outstanding,
                    // will initiate the view change protocol

                    let leader = self.state.current_leader();
                    let leader_addr = self.config.peer_addrs.get(&leader).unwrap();
                    let _ = self
                        .tx_node
                        .send(NodeCommand::SendMessageCommand(SendMessage {
                            destination: *leader_addr,
                            message: Message::ClientRequestMessage(request.clone()),
                        }))
                        .await;

                    // if we are adding
                    let newly_added = self.view_changer.add_to_wait_set(&request);
                    if newly_added {
                        let view_changer = self.view_changer.clone();
                        tokio::spawn(async move {
                            view_changer.wait_for(&request.clone()).await;
                        });
                    }
                }

                ConsensusCommand::InitPrePrepare(request) => {
                    // Here we are primary and received a client request which we deemed valid
                    // so we broadcast a Pre_prepare Message to the network and assign
                    // the next sequence number to this request
                    self.state.seq_num += 1;

                    let pre_prepare = PrePrepare::new_with_signature(
                        self.keypair_bytes.clone(),
                        self.id,
                        self.state.view,
                        self.state.seq_num,
                        &request,
                    );

                    self.view_changer
                        .add_to_sent_pre_prepares(&(pre_prepare.view, pre_prepare.seq_num));

                    let view_changer = self.view_changer.clone();
                    tokio::spawn(async move {
                        view_changer
                            .wait_for_sent_pre_prepares(&(pre_prepare.view, pre_prepare.seq_num))
                            .await;
                    });

                    let pre_prepare_message = Message::PrePrepareMessage(pre_prepare.clone());

                    let _ = self
                        .tx_node
                        .send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                            message: pre_prepare_message.clone(),
                        }))
                        .await;
                }

                ConsensusCommand::RebroadcastPrePrepare(view_seq_num_pair) => {
                    // we are the leader and a pre-prepare message we sent has not been execute for some time
                    // so we rebroadcast the message to the networks

                    info!(
                        "Rebroadcasting PrePrepare with seq-num {:?}",
                        view_seq_num_pair
                    );

                    let pre_prepare = self
                        .state
                        .message_bank
                        .accepted_pre_prepare_requests
                        .get(&view_seq_num_pair)
                        .unwrap()
                        .clone();
                    let pre_prepare_message = Message::PrePrepareMessage(pre_prepare.clone());
                    let _ = self
                        .tx_node
                        .send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                            message: pre_prepare_message.clone(),
                        }))
                        .await;
                    let view_changer = self.view_changer.clone();
                    tokio::spawn(async move {
                        view_changer
                            .wait_for_sent_pre_prepares(&(pre_prepare.view, pre_prepare.seq_num))
                            .await;
                    });
                }

                ConsensusCommand::AcceptPrePrepare(pre_prepare) => {
                    // We received a PrePrepare message from the network, and we see no violations
                    // So we will broadcast a corresponding prepare message and begin to count votes
                    info!("Accepted PrePrepare from {}", pre_prepare.id);
                    self.state
                        .message_bank
                        .accepted_pre_prepare_requests
                        .insert((pre_prepare.view, pre_prepare.seq_num), pre_prepare.clone());

                    let prepare = Prepare::new_with_signature(
                        self.keypair_bytes.clone(),
                        self.id,
                        pre_prepare.view,
                        pre_prepare.seq_num,
                        &pre_prepare.clone().client_request,
                    );

                    let prepare_message = Message::PrepareMessage(prepare.clone());
                    let _ = self
                        .tx_node
                        .send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                            message: prepare_message.clone(),
                        }))
                        .await;

                    self.state
                        .message_bank
                        .log
                        .push_back(Message::PrePrepareMessage(pre_prepare.clone()));

                    // we may already have a got a prepare message which we did not accept because
                    // we did not receive this pre-prepare message message yet
                    for e_prepare in self.state.message_bank.outstanding_prepares.iter() {
                        if e_prepare.corresponds_to(&pre_prepare) {
                            info!("Found outstanding prepare from {}", e_prepare.id);
                            let _ = self
                                .tx_consensus
                                .send(ConsensusCommand::AcceptPrepare(e_prepare.clone()))
                                .await;
                        }
                    }

                    // at this point, we need to trigger a timer, and if the timer expires
                    // and the request is still outstanding, then we need to trigger a view change
                    // as this is evidence that the system has stopped making progress
                    let newly_added = self
                        .view_changer
                        .add_to_wait_set(&pre_prepare.client_request);
                    if newly_added {
                        let view_changer = self.view_changer.clone();
                        tokio::spawn(async move {
                            view_changer.wait_for(&pre_prepare.client_request).await;
                        });
                    }
                }

                ConsensusCommand::AcceptPrepare(prepare) => {
                    // We saw a prepare message from the network that we deemed was valid
                    // to we increment the vote count, and if we have enough prepare votes
                    // then we move to the commit phases

                    info!("Accepted Prepare from {}", prepare.id);

                    // we are not accepting this prepare, so if it is our outstanding set, then
                    //we may remove it
                    self.state
                        .message_bank
                        .outstanding_prepares
                        .remove(&prepare);

                    // add the prepare message we are accepting to the log
                    self.state
                        .message_bank
                        .log
                        .push_back(Message::PrepareMessage(prepare.clone()));

                    // TODO: Move the prepare votes into the state struct
                    // Count votes for this prepare message and see if we have enough to move to the commit phases
                    if let Some(curr_vote_set) = self
                        .state
                        .prepare_votes
                        .get_mut(&(prepare.view, prepare.seq_num))
                    {
                        curr_vote_set.insert(prepare.id);
                        if curr_vote_set.len() > 2 * self.config.num_faulty {
                            // at this point, we have enough prepare votes to move into the commit phase.
                            let _ = self
                                .view_changer
                                .tx_consensus
                                .send(ConsensusCommand::EnterCommit(prepare.clone()))
                                .await;
                        }
                    } else {
                        // first time we got a prepare message for this view and sequence number
                        let mut new_vote_set = HashSet::new();
                        new_vote_set.insert(prepare.id);
                        self.state
                            .prepare_votes
                            .insert((prepare.view, prepare.seq_num), new_vote_set);
                    }

                    // we may already have a got a commit message which we did not accept because
                    // we did not receive this prepare message message yet
                    for e_commit in self.state.message_bank.outstanding_commits.iter() {
                        if e_commit.corresponds_to(&prepare) {
                            info!("Found outstanding commit from {}", e_commit.id);
                            let _ = self
                                .tx_consensus
                                .send(ConsensusCommand::AcceptCommit(e_commit.clone()))
                                .await;
                        }
                    }
                }

                ConsensusCommand::EnterCommit(prepare) => {
                    //todo make a new commit message builder

                    let commit = Commit::new_with_signature(
                        self.keypair_bytes.clone(),
                        self.id,
                        prepare.view,
                        prepare.seq_num,
                        prepare.client_request_digest,
                    );

                    let commit_message = Message::CommitMessage(commit);
                    let _ = self
                        .tx_node
                        .send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                            message: commit_message,
                        }))
                        .await;
                }

                ConsensusCommand::AcceptCommit(commit) => {
                    // We received a Commit Message for a request that we deemed valid
                    // so we increment the vote count

                    info!("Accepted commit from {}", commit.id);

                    self.state.message_bank.outstanding_commits.remove(&commit);

                    self.state
                        .message_bank
                        .log
                        .push_back(Message::CommitMessage(commit.clone()));

                    if let Some(curr_vote_set) = self
                        .state
                        .commit_votes
                        .get_mut(&(commit.view, commit.seq_num))
                    {
                        curr_vote_set.insert(commit.id);
                        if curr_vote_set.len() > 2 * self.config.num_faulty {
                            // At this point, we have enough commit votes to commit the message
                            let _ = self
                                .tx_consensus
                                .send(ConsensusCommand::ApplyCommit(commit))
                                .await;
                        }
                    } else {
                        // first time we got a prepare message for this view and sequence number
                        let mut new_vote_set = HashSet::new();
                        new_vote_set.insert(commit.id);
                        self.state
                            .commit_votes
                            .insert((commit.view, commit.seq_num), new_vote_set);
                    }
                }

                ConsensusCommand::InitViewChange(request) => {
                    if self.state.in_view_change || self.state.current_leader() == self.id {
                        // we are already in a view change state or we are currently the leader
                        continue;
                    }
                    info!("Initializing view change...");
                    self.state.in_view_change = true;
                }

                ConsensusCommand::ApplyCommit(commit) => {
                    // we now have permission to apply the client request
                    let client_request = self
                        .state
                        .message_bank
                        .accepted_pre_prepare_requests
                        .get(&(commit.view, commit.seq_num))
                        .unwrap()
                        .clone()
                        .client_request;

                    self.apply_commit(&commit, &client_request).await;
                    info!("New state: {}" ,self.state.last_seq_num_committed);
                    // The request we just committed was enough to now trigger a checkpoint
                    if self.state.last_seq_num_committed % self.config.checkpoint_frequency == 0
                        && self.state.last_seq_num_committed > self.state.last_stable_seq_num
                    {
                        // trigger the checkpoint process
                        // if a replica receives 2f + 1 of these checkpoints,
                        // it can begin to discard everythihng before the agreed sequence number. Also
                        // if the replica does not have the state up to that point, it can
                        // make a request to get caught up to speed from one of these replicas.

                        // create a checkpoint message and broadcast it
                        self.init_checkpoint().await;
                    }
                }

                ConsensusCommand::AcceptCheckpoint(checkpoint) => {
                    self.state
                        .message_bank
                        .log
                        .push_back(Message::CheckPointMessage(checkpoint.clone()));

                    // increment vote count for checkpoint with given committed seq num and given digest
                    if let Some(curr_vote_set) = self.state.checkpoint_votes.get_mut(&(
                        checkpoint.committed_seq_num,
                        checkpoint.state_digest.clone(),
                    )) {
                        curr_vote_set.insert(checkpoint.id);
                        if curr_vote_set.len() > 2 * self.config.num_faulty {
                            info!("Updating state from checkpoint");
                            // At this point, we have enough checkpoint messages to update out state
                            for (commit, client_request) in checkpoint.checkpoint_commits.iter() {
                                self.apply_commit(commit, client_request).await;
                            }

                            if self.state.last_seq_num_committed < checkpoint.committed_seq_num {
                                // if this node is still behind, we fast-forward its state
                                // but note that no client responses are sent.
                                self.state.store = checkpoint.state;
                                self.state.last_seq_num_committed = checkpoint.committed_seq_num;
                            }
                                                        

                            // todo: log this checkpoint certificate for future view changes

                            // update the stable seq num and garbage collect up to this seq num
                            self.state.last_stable_seq_num = checkpoint.committed_seq_num;
                            self.state.message_bank.garbage_collect(checkpoint.committed_seq_num);
                        }
                    } else {
                        // first time we got a prepare message for this view and sequence number
                        let mut new_vote_set = HashSet::new();
                        new_vote_set.insert(checkpoint.id);
                        self.state.checkpoint_votes.insert(
                            (checkpoint.committed_seq_num, checkpoint.state_digest),
                            new_vote_set,
                        );
                    }
                }
            }
        }
    }

    #[allow(clippy::comparison_chain)]
    pub async fn apply_commit(&mut self, commit: &Commit, client_request: &ClientRequest) {
        // remove this request from the view changer so that we don't trigger a view change
        self.view_changer.remove_from_wait_set(client_request);
        self.view_changer
            .remove_from_sent_pre_prepares(&(commit.view, commit.seq_num));

        if commit.seq_num == self.state.last_seq_num_committed + 1 {
            info!("Applying client request with seq_num {}", commit.seq_num);

            let (ret, new_applies) = self.state.apply_commit(client_request, commit);
            for commit in new_applies.iter() {
                let _ = self
                    .tx_consensus
                    .send(ConsensusCommand::ApplyCommit(commit.clone()))
                    .await;
            }

            
            // build the client response and send to client

            let res_val = if ret.is_some() {Some(*ret.unwrap().unwrap())} else {None};
            let res_success = res_val.is_some() || client_request.value.is_some();

            
            let client_response = ClientResponse::new_with_signature (
                self.id,
                client_request.time_stamp,
                client_request.key.clone(),
                res_val,
                res_success
            );

            let _ = self.tx_node.send(NodeCommand::SendMessageCommand(SendMessage {
                message: Message::ClientResponseMessage(client_response),
                destination: client_request.respond_addr
            })).await;


        } else if commit.seq_num > self.state.last_seq_num_committed + 1 {
            //the sequence number for this commit is too large, so we do not apply it yet
            if self
                .state
                .message_bank
                .accepted_commits_not_applied
                .insert(commit.seq_num, commit.clone())
                .is_none()
            {
                info!("Buffering client request with seq_num {}", commit.seq_num);
            }
        }
    }

    pub async fn init_checkpoint(&mut self) {
        info!("Initiating checkpoint");
        let mut checkpoint_commits = Vec::<(Commit, ClientRequest)>::new();
        for seq_num in self.state.last_stable_seq_num + 1..self.state.last_seq_num_committed + 1 {
            checkpoint_commits.push(
                self.state
                    .message_bank
                    .applied_commits
                    .get(&seq_num)
                    .unwrap()
                    .clone(),
            );
        }

        let checkpoint = CheckPoint::new(
            self.id,
            self.state.last_seq_num_committed,
            self.state.digest(),
            self.state.store.clone(),
            checkpoint_commits,
        );

        let _ = self
            .tx_node
            .send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                message: Message::CheckPointMessage(checkpoint),
            }))
            .await;
    }
}
