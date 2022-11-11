use crate::messages::Message;

use std::collections::VecDeque;

pub struct Consensus {
    view: usize,

    log: VecDeque<Message>,
}

impl Default for Consensus {
    fn default() -> Self {
        Self {
            view : 0,
            log : VecDeque::new(),
        }
    }
}

impl Consensus {

    pub fn add_to_log(&mut self, message : Message) {
        self.log.push_back(message);
    }

}