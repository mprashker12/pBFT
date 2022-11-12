use crate::config::Config;
use crate::consensus::Consensus;
use crate::messages::{Message, PrePrepare, Prepare};
use crate::{NodeId, Result};

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncWriteExt, BufStream};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, Duration, Instant};
use tokio::{io::AsyncBufReadExt, sync::Mutex};

// TODO: We may use a mpsc channel for the inner node to communicate with its parent node

pub struct Node {
    /// If of this ndoe
    pub id: NodeId,
    /// Configuration of Cluster this node is in
    pub config: Config,
    /// Socket on which this node is listening for connections from peers
    pub addr: SocketAddr,
    /// Node state which will be shared across Tokio tasks
    pub inner: InnerNode,
}

#[derive(Clone)]
pub struct InnerNode {
    /// Currently open connections maintained with other nodes for writing
    pub open_write_connections: Arc<Mutex<HashMap<SocketAddr, BufStream<TcpStream>>>>,
    /// Consensus engine
    pub consensus: Arc<Mutex<Consensus>>,
    /// Addresses of other nodes in the network
    pub peer_addrs: Arc<Vec<SocketAddr>>,
}

impl Node {
    pub fn new(id: NodeId, config: Config) -> Self {
        let addr_me = *config.listen_addrs.get(&id).unwrap();
        let mut addr_peers = Vec::<SocketAddr>::new();
        for (_, peer_addr) in config.listen_addrs.iter() {
            addr_peers.push(*peer_addr);
        }

        //todo : pass in config to consensus for construction
        // we will also have a mpsc channel for consensus to communicate with the node

        let inner = InnerNode {
            open_write_connections: Arc::new(Mutex::new(HashMap::new())),
            consensus: Arc::new(Mutex::new(Consensus::new(config.clone()))),
            peer_addrs: Arc::new(addr_peers),
        };

        Self {
            id,
            config,
            addr: addr_me,
            inner,
        }
    }

    pub async fn run(&mut self) {
        let listener = TcpListener::bind(self.addr).await.unwrap();
        let peer_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8079);

        println!("Node {} listening on {}", self.id, self.addr);

        let timer = sleep(Duration::from_secs(4));
        tokio::pin!(timer);

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

                () = &mut timer => {
                    // timer expired
                    let message = Message::PrePrepareMessage(PrePrepare {
                        view: 100,
                        seq_num: 101,
                        digest: 102,
                    });
                    let inner = self.inner.clone();
                    // reset the timer
                    timer.as_mut().reset(Instant::now() + Duration::from_secs(4));
                    tokio::spawn(async move {
                        let mut should_remove : bool = false;
                        if let Err(e) = inner.send_message(&peer_addr, message).await {
                            println!("Failed to connect to peer {}", e);
                            should_remove = true;
                        }
                        if should_remove {
                            inner.open_write_connections.lock().await.remove(&peer_addr);
                        }
                    });
                    let message = Message::PrePrepareMessage(PrePrepare {
                        view: 104,
                        seq_num: 105,
                        digest: 106,
                    });

                    if self.id == 2 {
                        self.inner.broadcast(message).await;
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
            match message {
                Message::PrePrepareMessage(pre_prepare) => {
                    self.handle_pre_prepare(pre_prepare).await;
                }
                Message::PrepareMessage(prepare) => {
                    self.handle_prepare(prepare).await;
                }
            }
        }
    }

    pub async fn broadcast(&self, message: Message) {
        for peer_addr in self.peer_addrs.iter() {
            let _ = self.send_message(peer_addr, message).await;
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
        let _bytes_written = stream
            .get_mut()
            .write(message.serialize().as_slice())
            .await?;
        Ok(())
    }

    async fn handle_pre_prepare(&self, pre_prepare: PrePrepare) {
        let mut consensus = self.consensus.lock().await;

        if consensus.should_accept_pre_prepare(&pre_prepare) {
            // if we accept, we should broadcast to the network a corresponding prepare message
            // and add both messages to the log. Otherwise, we do nothing. The consensus struct has
            // all information needed to determine if we should accept the pre-prepare
            consensus.add_to_log(&Message::PrePrepareMessage(pre_prepare));
        }
    }

    async fn handle_prepare(&self, prepare: Prepare) {
        let mut consensus = self.consensus.lock().await;
    }
}
