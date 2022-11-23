use crate::config::Config;
use crate::consensus::Consensus;
use crate::messages::{
    ClientRequest, ConsensusCommand, Identifier, Message, NodeCommand, PrePrepare, Prepare,
};
use crate::{NodeId, Result};

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncWriteExt, BufStream};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{sleep, Duration, Instant};
use tokio::{io::AsyncBufReadExt, sync::Mutex};

use ed25519_dalek::{Keypair, PublicKey, SecretKey};

use env_logger::Env;
use log::{debug, error, info, log_enabled, Level};

// TODO: We may use a mpsc channel for the inner node to communicate with its parent node

pub struct Node {
    /// Id of this node
    pub id: NodeId,
    /// Configuration of Cluster this node is in
    pub config: Config,
    /// Socket on which this node is listening for connections from peers
    pub addr: SocketAddr,
    /// Node state which will be shared across Tokio tasks
    pub inner: InnerNode,
    /// Receive Commands from the Consensus Engine
    pub rx_node: Receiver<NodeCommand>,
}

#[derive(Clone)]
pub struct InnerNode {
    /// Id of the outer node
    pub id: NodeId,
    /// Config of the cluster of the outer node
    pub config: Config,
    /// Keypair of this node used to sign messages
    pub keypair_bytes: Vec<u8>,
    /// Public key of this node
    pub pub_key: PublicKey,
    /// Known public keys of peers
    pub peer_pub_keys: Arc<Mutex<HashMap<NodeId, PublicKey>>>,
    /// Send Consensus Commands to Consensus engine
    pub tx_consensus: Sender<ConsensusCommand>,
    /// Send Node Commands to itself
    pub tx_node: Sender<NodeCommand>,
}

impl Node {
    pub fn new(
        id: NodeId,
        config: Config,
        keypair_bytes: Vec<u8>,
        pub_key: PublicKey,
        rx_node: Receiver<NodeCommand>,
        tx_consensus: Sender<ConsensusCommand>,
        tx_node: Sender<NodeCommand>,
    ) -> Self {
        let mut logger = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
        logger.init();

        let addr_me = *config.peer_addrs.get(&id).unwrap();

        let inner = InnerNode {
            id,
            config: config.clone(),
            keypair_bytes,
            pub_key,
            peer_pub_keys: Arc::new(Mutex::new(HashMap::new())),
            tx_consensus,
            tx_node,
        };

        Self {
            id,
            config,
            addr: addr_me,
            inner,
            rx_node,
        }
    }

    pub async fn spawn(&mut self) {
        let listener = TcpListener::bind(self.addr).await.unwrap();
        info!("Node {} listening on {}", self.id, self.addr);

        // We periodically broadcast our identity to all of the other nodes in the network
        let inner = self.inner.clone();
        tokio::spawn(async move {
            loop {
                inner
                    .broadcast(&Message::IdentifierMessage(Identifier {
                        id: inner.id,
                        pub_key_vec: inner.pub_key.as_bytes().to_vec(),
                    }))
                    .await;
                sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        loop {
            tokio::select! {
                // future representing an incoming connection
                // we maintain the connection and only read from it
                // perhaps updating the consensus state
                res = listener.accept() => {
                    let (mut stream, _) = res.unwrap();
                    let inner = self.inner.clone();
                    tokio::spawn(async move {
                        if let Err(e) = inner.read_message(&mut stream).await {
                            println!("Unable to read message from incoming connection {}", e);
                        }
                    });
                }

                // make a future representing an incoming message from the consensus engine
                res = self.rx_node.recv() => {
                    let cmd = res.unwrap();
                    match cmd {
                        NodeCommand::SendMessageCommand(send_message) => {
                            let _ = self.inner.send_message(&send_message.destination, send_message.message).await;
                        }
                        NodeCommand::BroadCastMessageCommand(broadcast_message) => {
                            self.inner.broadcast(&broadcast_message.message).await;
                        }
                    }
                }
            }
        }
    }
}

impl InnerNode {
    pub async fn read_message(&self, stream: &mut TcpStream) -> Result<()> {
        let mut reader = BufStream::new(stream);
        let mut buf = String::new();
        let _ = reader.read_line(&mut buf).await?;
        let message: Message = serde_json::from_str(&buf)?;
        //println!("Received {:?} from {}", message, peer_addr);

        if let Message::IdentifierMessage(identifier) = message.clone() {
            // we received an identifier message from another node
            // so we record their public key and we do not pass the message to consensus
            let mut peer_pub_keys = self.peer_pub_keys.lock().await;
            let peer_id = identifier.id;
            let peer_pub_key = PublicKey::from_bytes(identifier.pub_key_vec.as_slice()).unwrap();
            //println!("Received identifier {:?}", peer_id);
            peer_pub_keys.insert(peer_id, peer_pub_key);
            return Ok(());
        } else if self.should_drop(&message).await {
            println!("Dropping message from {:?}", message.get_id());
            return Ok(());
        }

        let _ = self
            .tx_consensus
            .send(ConsensusCommand::ProcessMessage(message.clone()))
            .await;
        Ok(())
    }

    pub async fn broadcast(&self, message: &Message) {
        for (_, peer_addr) in self.config.peer_addrs.iter() {
            let _ = self.send_message(peer_addr, message.clone()).await;
        }
    }

    // all of our write streams should be taking place through the streams in the open_write_connections
    pub async fn send_message(
        &self,
        peer_addr: &SocketAddr,
        message: Message,
    ) -> crate::Result<()> {
        //println!("Sending message {:?} to {:?}", message, peer_addr);

        let mut stream = BufStream::new(TcpStream::connect(peer_addr).await?);
        if let Err(e) = stream.get_mut().write(message.serialize().as_slice()).await {
            println!("Failed to send to {}", peer_addr);
            return Err(Box::new(e));
        }
        Ok(())
    }

    pub async fn should_drop(&self, message: &Message) -> bool {
        if let Message::ClientRequestMessage(_) = message {
            // we consider client requests to always be properly signed
            return false;
        }

        let peer_pub_keys = self.peer_pub_keys.lock().await;
        let peer_id = message.get_id().unwrap();
        if peer_pub_keys.get(&peer_id).is_none() {
            // No public key found for peer id so we drop the message
            return true;
        }
        let peer_pub_key = peer_pub_keys.get(&peer_id).unwrap();
        if !message.is_properly_signed_by(peer_pub_key) {
            // The message is not properly signed so we drop it
            return true;
        }
        false
    }
}
