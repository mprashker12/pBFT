/// Byzantine Fault Tolerant KV-Store

pub type NodeId = usize;

pub type Key = String;
pub type Value = u32;

pub mod config;
pub mod messages;
pub mod node;
pub mod consensus;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {}
}
