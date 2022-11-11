use crate::messages::{Message, PrePrepare};
use crate::{Result, NodeId};
use crate::consensus::Consensus;
use crate::config::Config;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::{BufStream, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::{
    io::{AsyncBufReadExt},
    sync::{Mutex},
};
use tokio::time::{sleep, Duration, Instant};

pub struct Node {
    /// If of this ndoe
    pub id : NodeId,
    /// Configuration of Cluster this node is in
    pub config : Config,
    /// Socket on which this node is listening for connections from peers
    pub addr: SocketAddr,
    /// Node state which will be shared across Tokio tasks
    pub inner: InnerNode,
}

#[derive(Clone)]
pub struct InnerNode {
    /// Note that these connections will only be used for writing
    pub open_write_connections : Arc<Mutex<HashMap<SocketAddr, BufStream<TcpStream>>>>,

    pub consensus : Arc<Mutex<Consensus>>,
}

impl Node {
    pub fn new(id : NodeId, config: Config) -> Self {
        
        let addr =*config.listen_addrs.get(&id).unwrap(); 

        let inner = InnerNode { 
            open_write_connections : Arc::new(Mutex::new(HashMap::new())),
            consensus: Arc::new(Mutex::new(Consensus::default())),
        };
        
        Self {
            id,
            config,
            addr,
            inner,
        }
    }

    pub async fn run(&mut self) {
        let listener = TcpListener::bind(self.addr).await.unwrap();
        let peer_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8079);

        
        let timer = sleep(Duration::from_secs(4));
        tokio::pin!(timer);

        loop {
            tokio::select! {
                
                // future representing an incoming connection
                // we maintain the connection and only read from it
                // perhaps updating the consensus state
                res = listener.accept() => {
                    let (mut stream, _) = res.unwrap();
                    let mut inner = self.inner.clone();
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
                }

            }
        }
    }
}

impl InnerNode {

    pub async fn insert_write_connection(&mut self, stream : TcpStream) {
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
                println!("Incoming read connection from {:?} has been terminated", peer_addr);
                return Ok(())
            }
            let message: Message = serde_json::from_str(&buf)?;
            {
                //todo: Make this a separate function
                self.consensus.lock().await.add_to_log(message);
            }
            match message {
                Message::PrePrepareMessage(PrePrepare) => {

                }
            }
        }
    }

    // all of our write streams should be taking place through the streams in the open_write_connections
    pub async fn send_message(
        &self,
        peer_addr: &SocketAddr,
        message: Message,
    ) -> crate::Result<()> {
        println!("Sending message");
        let mut connections = self.open_write_connections.lock().await;
        if let std::collections::hash_map::Entry::Vacant(e) = connections.entry(*peer_addr) {
            let new_stream = BufStream::new(TcpStream::connect(peer_addr).await?);
            e.insert(new_stream);
        }

        let mut serialized_message = serde_json::to_string(&message).unwrap();
        serialized_message.push('\n');
    

        let stream = connections.get_mut(peer_addr).unwrap();
        println!("Sending {:?}", serialized_message);
        let bytes_written = stream.get_mut().write(serialized_message.as_bytes()).await?;
        println!("{}", bytes_written);
        Ok(())
    }
}
