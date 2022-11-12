use crate::NodeId;
use crate::config::Config;
use crate::messages::{Message, PrePrepare, Prepare, ClientRequest, ConsensusCommand, NodeCommand};

use tokio::sync::mpsc::{Sender, Receiver};

use std::collections::VecDeque;


pub struct Consensus {
    /// Configuration of the cluster this node is in
    pub config: Config,
    /// Current view which this node believes the system is in
    pub view: usize,
    /// Next sequence number to assign to an incoming client reqest
    pub seq_num: usize,
    /// Log of messages this node maintains
    pub log: VecDeque<Message>,
    /// Receiver of Consensus Commands
    pub rx_consensus : Receiver<ConsensusCommand>,
    /// Sender of Consensus Commands 
    pub tx_consensus : Sender<ConsensusCommand>,
    /// Send Commands to Node
    pub tx_node : Sender<NodeCommand>,

}

impl Consensus {
    pub fn new(config: Config, rx_consensus : Receiver<ConsensusCommand>, tx_consensus : Sender<ConsensusCommand>, tx_node : Sender<NodeCommand>) -> Self {
        Self {
            config,
            view: 0,
            seq_num: 0,
            log: VecDeque::<Message>::new(),
            rx_consensus,
            tx_consensus,
            tx_node,
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
                            //we may make this its own tokio task
                            self.process_message(&message).await;
                        }
                    }
                }

            }
        }
    }

    pub async fn process_message(&mut self, message: &Message) {}

    pub fn current_leader(&self) -> NodeId {
        self.view % self.config.num_nodes
    }

    pub fn add_to_log(&mut self, message: &Message) {
        self.log.push_back(message.clone());
    }

    pub fn should_accept_pre_prepare(&self, message: &PrePrepare) -> bool {
        if self.view != message.view {return false;}
        // check if we have already seen a sequence number for this view
        true
    }

    pub fn should_accept_prepare(&self, message: &Prepare) -> bool {
        if self.view != message.view {return false;}

        true
    }

    pub fn process_client_request(&mut self, client_request: &ClientRequest) {}
}
