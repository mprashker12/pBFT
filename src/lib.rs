/// Byzantine Fault Tolerant KV-Store
///

pub type Key = String;
pub type Value = u32;

pub mod messages;
pub mod net_sender;
pub mod node;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {}
}
