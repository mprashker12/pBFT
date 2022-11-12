use crate::config::Config;
use crate::consensus::Consensus;
use crate::messages::{ClientRequest, ConsensusCommand, Message, NodeCommand, PrePrepare, Prepare};
use crate::{NodeId, Result};

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncWriteExt, BufStream};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{sleep, Duration, Instant};
use tokio::{io::AsyncBufReadExt, sync::Mutex};

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
    /// Currently open connections maintained with other nodes for writing
    pub open_write_connections: Arc<Mutex<HashMap<SocketAddr, BufStream<TcpStream>>>>,
    /// Consensus engine
    pub tx_consensus: Sender<ConsensusCommand>,
    /// Send Node Commands to itself (used internally)
    pub tx_node: Sender<NodeCommand>,
}

impl Node {
    pub fn new(
        id: NodeId,
        config: Config,
        rx_node: Receiver<NodeCommand>,
        tx_consensus: Sender<ConsensusCommand>,
        tx_node: Sender<NodeCommand>,
    ) -> Self {
        let addr_me = *config.peer_addrs.get(&id).unwrap();

        // todo: we may also have a mpsc channel for consensus to communicate with the node

        let inner = InnerNode {
            id,
            config: config.clone(),
            open_write_connections: Arc::new(Mutex::new(HashMap::new())),
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
        println!("Node {} listening on {}", self.id, self.addr);

        loop {
            tokio::select! {
                // future representing an incoming connection
                // we maintain the connection and only read from it
                // perhaps updating the consensus state
                res = listener.accept() => {
                    let (mut stream, _) = res.unwrap();
                    let inner = self.inner.clone();
                    tokio::spawn(async move {
                        if let Err(e) = inner.handle_connection(&mut stream).await {
                            println!("Incoming connection terminated {}", e);
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
    pub async fn insert_write_connection(&mut self, stream: TcpStream) {
        let mut connections = self.open_write_connections.lock().await;
        let peer_addr = stream.peer_addr().unwrap();
        let buf_stream = BufStream::new(stream);
        connections.insert(peer_addr, buf_stream);
    }

    pub async fn handle_connection(&self, stream: &mut TcpStream) -> Result<()> {
        let peer_addr = stream.peer_addr().unwrap();
        let mut reader = BufStream::new(stream);
        loop {
            let mut buf = String::new();
            let bytes_read = reader.read_line(&mut buf).await?;
            if bytes_read == 0 {
                println!(
                    "Incoming read connection from {:?} has been terminated",
                    peer_addr
                );
                return Ok(());
            }
            let message: Message = serde_json::from_str(&buf)?;
            println!("Received {:?} from {}", message, peer_addr);
            let _ = self
                .tx_consensus
                .send(ConsensusCommand::ProcessMessage(message.clone()))
                .await;

            if let Message::ClientRequestMessage(_) = message {
                // we do not want to maintain persistent connections with each client connection
                // so we terminate the connection upon receiving a client request
                return Ok(());
            }
        }
    }

    pub async fn broadcast(&self, message: &Message) {
        for (node_id, peer_addr) in self.config.peer_addrs.iter() {
            if *node_id == self.id {continue;}
            let _ = self.send_message(peer_addr, message.clone()).await;
        }
    }

    // all of our write streams should be taking place through the streams in the open_write_connections
    pub async fn send_message(
        &self,
        peer_addr: &SocketAddr,
        message: Message,
    ) -> crate::Result<()> {
        println!("Sending message {:?} to {:?}", message, peer_addr);
        let mut connections = self.open_write_connections.lock().await;
        if let std::collections::hash_map::Entry::Vacant(e) = connections.entry(*peer_addr) {
            let new_stream = BufStream::new(TcpStream::connect(peer_addr).await?);
            e.insert(new_stream);
        }

        let stream = connections.get_mut(peer_addr).unwrap();
        if let Err(e) = stream
            .get_mut()
            .write(message.serialize().as_slice())
            .await {
                println!("Failed to send to {}", peer_addr);
                connections.remove(peer_addr);
                return Err(Box::new(e));
            }
        Ok(())
    }
}
