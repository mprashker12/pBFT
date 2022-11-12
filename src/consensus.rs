use crate::config::Config;
use crate::messages::{Message, PrePrepare};

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

    pub fn add_to_log(&mut self, message: &Message) {
        self.log.push_back(*message);
    }

    pub fn should_accept_pre_prepare(&self, message: &PrePrepare) -> bool {
        true
    }
}
