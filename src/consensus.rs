use crate::NodeId;
use crate::config::Config;
use crate::messages::{Message, PrePrepare, Prepare, ClientRequest};

use std::collections::VecDeque;


pub struct Consensus {
    view: usize,

    config: Config,

    log: VecDeque<Message>,
}

impl Consensus {
    pub fn new(config: Config) -> Self {
        Self {
            view: 0,
            config,
            log: VecDeque::<Message>::new(),
        }
    }

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
