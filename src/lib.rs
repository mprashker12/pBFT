/// Byzantine Fault Tolerant KV-Store 
/// 

pub type Key = String;
pub type Value = u32;


pub mod node;
pub mod net_receiver;
pub mod net_sender;
pub mod messages;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {}
}
