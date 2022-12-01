use crate::config::Config;
use crate::messages::{
    BroadCastMessage, CheckPoint, ClientRequest, ClientResponse, Commit, ConsensusCommand, Message,
    NewView, NodeCommand, PrePrepare, Prepare, SendMessage, ViewChange,
};
use crate::state::State;
use crate::view_changer::{ViewChanger};
use crate::NodeId;

use tokio::sync::mpsc::{Receiver, Sender};

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use log::{info};

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
            id,
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
                            // Identifier messages are not passed to the consensus engine
                            unreachable!()
                        }

                        Message::PrePrepareMessage(pre_prepare) => {
                            //info!("Saw preprepare from {}", pre_prepare.id);
                            if self.state.should_accept_pre_prepare(&pre_prepare) {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::AcceptPrePrepare(pre_prepare))
                                    .await;
                            }
                        }
                        Message::PrepareMessage(prepare) => {
                            //info!("Saw prepare from {}", prepare.id);
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
                            //info!("Saw commit from {}", commit.id);
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

                        Message::ViewChangeMessage(view_change) => {
                            if self.state.should_accept_view_change(&view_change) {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::AcceptViewChange(view_change))
                                    .await;
                            }
                        }

                        Message::NewViewMessage(new_view) => {
                            if self.state.should_accept_new_view(&new_view) {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::AcceptNewView(new_view))
                                    .await;
                            }
                        }

                        Message::CheckPointMessage(checkpoint) => {
                            info!("Saw checkpoint from {} {} {:?} {:?}", checkpoint.id, checkpoint.committed_seq_num, checkpoint.state, checkpoint.state_digest);

                            if self.state.should_accept_checkpoint(&checkpoint) {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::AcceptCheckpoint(checkpoint))
                                    .await;
                            }
                        }

                        Message::ClientRequestMessage(client_request) => {
                            //info!("Saw client request");
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
                            // we should never receive a client response message, so we ignore
                            continue;
                        }
                    }
                }

                ConsensusCommand::MisdirectedClientRequest(request) => {
                    // If we get a client request but are not the leader
                    // we forward the request to the leader. We started a timer
                    // which, if it expires and the request is still outstanding,
                    // will initiate the view change protocol

                    if self.state.message_bank.sent_requests.contains(&(self.state.view, request.clone())) {
                        continue;
                    }

                    if self.config.is_equivocator {
                        // if we are leader and are malicious, we don't wont to forward
                        // any client requests after the system kicks us out.
                        continue;
                    }

                    self.state
                        .message_bank
                        .sent_requests
                        .insert((self.state.view, request.clone()));
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
                    if self.state.message_bank.sent_requests.contains(&(self.state.view, request.clone())) {
                        continue;
                    }

                    self.state.seq_num += 1;

                    if self.config.is_equivocator {
                        // this node is an equivocator, so we send
                        // different messages to different nodes
                        self.equivocate_pre_prepare(request).await;
                        continue;
                    }

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

                    self.state
                        .message_bank
                        .sent_requests
                        .insert((self.state.view, request.clone()));
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
                    let pre_prepare= self
                        .state
                        .message_bank
                        .accepted_pre_prepare_requests
                        .get(&view_seq_num_pair);

                    if pre_prepare.is_none() {return;}
                    let pre_prepare = pre_prepare.unwrap().clone();
                        

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
                    //info!("Accepted PrePrepare from {}", pre_prepare.id);
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

                    //info!("Accepted Prepare from {}", prepare.id);

                    // we are now accepting this prepare, so if it is our outstanding set, then
                    // we may remove it
                    self.state
                        .message_bank
                        .outstanding_prepares
                        .remove(&prepare);

                    // TODO: Move the prepare votes into the state struct
                    // Count votes for this prepare message and see if we have enough to move to the commit phases
                    if let Some(curr_vote_set) = self
                        .state
                        .prepare_votes
                        .get_mut(&(prepare.view, prepare.seq_num))
                    {
                        curr_vote_set.insert(prepare.id, prepare.clone());
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
                        let mut new_vote_set = HashMap::<usize, Prepare>::new();
                        new_vote_set.insert(prepare.id, prepare.clone());
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

                    self.state.message_bank.outstanding_commits.remove(&commit);

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

                ConsensusCommand::InitViewChange(_request) => {
                    if self.state.in_view_change || self.state.current_leader() == self.id {
                        // we are already in a view change state or we are currently the leader
                        continue;
                    }
                    self.state.in_view_change = true;

                    // find all pre-prepares that we have at least 2f + 1 votes for that occurred after the last stable seq-num

                    let mut subsequent_prepares =
                        HashMap::<usize, (PrePrepare, Vec<Prepare>)>::new();
                    for ((view, seq_num), pre_prepare) in
                        self.state.message_bank.accepted_pre_prepare_requests.iter()
                    {
                        if *seq_num <= self.state.last_stable_seq_num {
                            // only consider requests with seq_num which come after the last stable seq-num
                            continue;
                        }
                        if let Some(vote_set) = self.state.prepare_votes.get(&(*view, *seq_num)) {
                            if vote_set.len() > 2 * self.config.num_faulty {
                                subsequent_prepares.insert(
                                    *seq_num,
                                    (
                                        pre_prepare.clone(),
                                        vote_set
                                            .clone()
                                            .into_iter()
                                            .map(|(_, prepare)| prepare)
                                            .collect(),
                                    ),
                                );
                            }
                        }
                    }

                    let view_change = ViewChange::new_with_signature(
                        self.keypair_bytes.clone(),
                        self.id,
                        self.state.view + 1,
                        self.state.last_stable_seq_num,
                        self.state.last_checkpoint_proof.clone(),
                        subsequent_prepares,
                    );

                    let _ = self
                        .tx_node
                        .send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                            message: Message::ViewChangeMessage(view_change),
                        }))
                        .await;
                }

                ConsensusCommand::AcceptViewChange(view_change) => {
                    // update the vote count
                    // if there are enough votes (and we are the primary for the next view)
                    // then we broadcast a corresponding new_view message
                    self.state
                        .view_change_votes
                        .insert(view_change.id, view_change.clone());
                    if self.state.view_change_votes.len() > 2 * self.config.num_faulty {
                        // broadcast a new view message

                        let mut view_change_messages = Vec::<ViewChange>::new();
                        for (_, view_change) in self.state.view_change_votes.iter() {
                            view_change_messages.push(view_change.clone());
                        }

                        let mut latest_stable_seq_num = self.state.last_stable_seq_num;
                        let mut max_seq_num = self.state.last_stable_seq_num;
                        for view_change in view_change_messages.iter() {
                            latest_stable_seq_num = std::cmp::max(
                                latest_stable_seq_num,
                                view_change.last_stable_seq_num,
                            );
                            for (seq_num, _) in view_change.subsequent_prepares.iter() {
                                max_seq_num = std::cmp::max(max_seq_num, *seq_num);
                            }
                        }

                        let mut outstanding_pre_prepares = Vec::<PrePrepare>::new();
                        self.state.seq_num = latest_stable_seq_num;
                        for seq_num in latest_stable_seq_num + 1..max_seq_num + 1 {
                            let mut pre_prepare_highest_view_at_seq_num: Option<PrePrepare> = None;
                            for view_change in view_change_messages.iter() {
                                if let Some((pre_prepare, _)) =
                                    view_change.subsequent_prepares.get(&seq_num)
                                {
                                    if pre_prepare_highest_view_at_seq_num.clone().is_none()
                                        || pre_prepare.view
                                            > pre_prepare_highest_view_at_seq_num
                                                .clone()
                                                .unwrap()
                                                .view
                                    {
                                        pre_prepare_highest_view_at_seq_num =
                                            Some(pre_prepare.clone());
                                    }
                                }
                            }

                            let new_pre_prepare_for_view =
                                if let Some(e_pre_prepare) = pre_prepare_highest_view_at_seq_num {
                                    PrePrepare::new_with_signature(
                                        self.keypair_bytes.clone(),
                                        self.id,
                                        self.state.view + 1,
                                        seq_num,
                                        &e_pre_prepare.client_request.clone(),
                                    )
                                } else {
                                    // create a pre-prepare with a no-op request
                                    // to fill in gaps in sequence number
                                    PrePrepare::new_with_signature(
                                        self.keypair_bytes.clone(),
                                        self.id,
                                        self.state.view + 1,
                                        seq_num,
                                        &ClientRequest::no_op(),
                                    )
                                };
                            outstanding_pre_prepares.push(new_pre_prepare_for_view);
                        }

                        let new_view = NewView::new_with_signature(
                            self.keypair_bytes.clone(),
                            self.id,
                            view_change.new_view,
                            view_change_messages,
                            outstanding_pre_prepares.clone(),
                        );


                        let _ = self
                            .tx_node
                            .send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                                message: Message::NewViewMessage(new_view),
                            }))
                            .await;
                    }
                }

                ConsensusCommand::AcceptNewView(new_view) => {
                    
                    self.state.in_view_change = false;
                    self.state.checkpoint_votes.clear();
                    self.state.view = new_view.view;

                    info!("Moving to view {}", new_view.view);
                    if self.state.current_leader() == self.id {
                        info!("I AM NEW LEADER (Node {})", self.id);
                    }


                    if self.state.current_leader() == self.id {
                        // if we are the leader in this new view, 
                        // then we need outstanding pre-prepares in this new view
                        for pre_prepare in new_view.outstanding_pre_prepares.iter() {
                            let _ = self
                                .tx_consensus
                                .send(ConsensusCommand::InitPrePrepare(
                                    pre_prepare.clone().client_request,
                                ))
                                .await;
                        }
                        for request in self.view_changer.wait_set().iter() {
                            println!("issuing old {:?}", request);
                            let _ = self.tx_consensus.send(ConsensusCommand::InitPrePrepare(request.clone())).await;
                        }
                    }
                    
                    self.view_changer.reset();
                  
                }

                ConsensusCommand::ApplyCommit(commit) => {
                    // we now have permission to apply the client request
                    let pre_prepare = self
                        .state
                        .message_bank
                        .accepted_pre_prepare_requests
                        .get(&(commit.view, commit.seq_num));

                    if pre_prepare.is_none() {
                        continue;
                    }
                    let client_request = pre_prepare.unwrap().clone().client_request;

                    self.apply_commit(&commit, &client_request).await;
                    info!("Current State: {}: {:?}", self.state.last_seq_num_committed, self.state.store);

                    // The request we just committed was enough to now trigger a checkpoint
                    if self.state.last_seq_num_committed % self.config.checkpoint_frequency == 0
                        && self.state.last_seq_num_committed > self.state.last_stable_seq_num
                    {
                        // The request we just committed was enough to now trigger a checkpoint
                        self.init_checkpoint().await;
                    }
                }

                ConsensusCommand::AcceptCheckpoint(checkpoint) => {
                    self.state.message_bank.checkpoint_messages.insert(
                        (
                            checkpoint.committed_seq_num,
                            checkpoint.state_digest.clone(),
                        ),
                        checkpoint.clone(),
                    );

                    self.state
                        .checkpoints_current_round
                        .insert(checkpoint.id, checkpoint.clone());

                    // increment vote count for checkpoint with given committed seq num and given digest
                    if let Some(curr_vote_set) = self.state.checkpoint_votes.get_mut(&(
                        checkpoint.committed_seq_num,
                        checkpoint.state_digest.clone(),
                    )) {
                        curr_vote_set.insert(checkpoint.id);
                
                        if curr_vote_set.len() >= 2 * self.config.num_faulty {
                            // At this point, we have enough checkpoint messages to update out state
                            info!("Updating state from checkpoint");

                            if self.state.last_seq_num_committed < checkpoint.committed_seq_num {
                                // if this node is still behind after applying all commits in the checkpoint,
                                // we fast-forward its state, but note that no client responses are sent.
                                self.state.store = checkpoint.state;
                                self.state.last_seq_num_committed = checkpoint.committed_seq_num;
                            }

                            // make a new proof of this checkpoint for subsequent view change messages
                            self.state.update_checkpoint_meta(
                                &checkpoint.committed_seq_num,
                                &checkpoint.state_digest.clone(),
                            );

                            // update the stable seq num
                            self.state.last_stable_seq_num = checkpoint.committed_seq_num;

                            // we update the view to the largest sequence number in the commits
                            // in the checkpoint
                            let new_view = checkpoint.view;

                            if new_view != self.state.view {
                                // if we update to a new view,
                                // then we need to reset any view change processes
                                // which we initiated
                                self.state.in_view_change = false;
                                self.view_changer.reset();
                            }

                            self.state.view = new_view;

                            for commit in self.state.get_next_consecutive_commits().iter() {
                                let _ = self
                                    .tx_consensus
                                    .send(ConsensusCommand::ApplyCommit(commit.clone()))
                                    .await;
                            }

                            // remove all of the messages pertaining to requests with seq_num < last_stable_seq_num
                            self.state.garbage_collect();
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
            info!(
                "Applying client request with view {} seq-num {}",
                commit.view, commit.seq_num
            );

            let (ret, new_applies) = self.state.apply_commit(client_request, commit);
            for commit in new_applies.iter() {
                let _ = self
                    .tx_consensus
                    .send(ConsensusCommand::ApplyCommit(commit.clone()))
                    .await;
            }

            // build the client response and send to client

            let res_val = if ret.is_some() { ret.unwrap() } else { None };
            //let res_success = res_val.is_some() || client_request.value.is_some();
            let res_success = true;

            let client_response = if res_val.is_some() {
                ClientResponse::new_with_signature(
                    self.id,
                    client_request.time_stamp,
                    client_request.key.clone(),
                    Some(*(res_val.unwrap())),
                    res_success,
                )
            } else {
                ClientResponse::new_with_signature(
                    self.id,
                    client_request.time_stamp,
                    client_request.key.clone(),
                    None,
                    res_success,
                )
            };

            let _ = self
                .tx_node
                .send(NodeCommand::SendMessageCommand(SendMessage {
                    message: Message::ClientResponseMessage(client_response),
                    destination: client_request.respond_addr,
                }))
                .await;
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

        let checkpoint = CheckPoint::new_with_signature(
            self.keypair_bytes.clone(),
            self.id,
            self.state.last_seq_num_committed,
            self.state.view,
            self.state.digest(),
            self.state.store.clone(),
        );

        let _ = self
            .tx_node
            .send(NodeCommand::BroadCastMessageCommand(BroadCastMessage {
                message: Message::CheckPointMessage(checkpoint),
            }))
            .await;
    }

    async fn equivocate_pre_prepare(&self, request: ClientRequest) {
        
        // mutate the given request
        let mut d_request = request.clone();
        d_request.value = Some(42);
        
        let pre_prepare = PrePrepare::new_with_signature(
            self.keypair_bytes.clone(),
            self.id,
            self.state.view,
            self.state.seq_num,
            &request,
        );

        let d_pre_prepare = PrePrepare::new_with_signature(
            self.keypair_bytes.clone(),
            self.id,
            self.state.view,
            self.state.seq_num,
            &d_request,
        );

        let pre_prepare_message = Message::PrePrepareMessage(pre_prepare.clone());
        let d_pre_prepare_message = Message::PrePrepareMessage(d_pre_prepare.clone());
        
        // if the peer-id is even, we send the original request, otherwise, 
        // we send the differentiated request. 
        for (peer_id, peer_addr) in self.config.peer_addrs.iter() {
            if peer_id % 2 == 0 {
                let _ = self.tx_node.send(NodeCommand::SendMessageCommand(SendMessage {
                    destination: *peer_addr,
                    message: d_pre_prepare_message.clone(),
                })).await;
            } else {
                let _ = self.tx_node.send(NodeCommand::SendMessageCommand(SendMessage {
                    destination: *peer_addr,
                    message: pre_prepare_message.clone(),
                })).await;
            }
        }
    }
}
