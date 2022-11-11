use crate::messages::Message;

use std::collections::VecDeque;

pub struct State {
    view: usize,

    log: VecDeque<Message>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            view : 0,
            log : VecDeque::new(),
        }
    }
}

impl State {

    pub fn add_to_log(&mut self, message : Message) {
        self.log.push_back(message);
    }

}