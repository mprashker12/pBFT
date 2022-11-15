use crate::messages::Message;
use crate::{Key, Value};
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub struct State {
    pub in_view_change: bool,
    pub view: usize,
    pub seq_num: usize,
    pub last_seq_num_committed: usize,
    pub log: VecDeque<Message>,
    pub store: HashMap<Key, Value>,
}
