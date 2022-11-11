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
    pub open_connections : Arc<Mutex<HashMap<SocketAddr, BufStream<TcpStream>>>>,

    pub state : Arc<Mutex<Consensus>>,
}

impl Node {
    pub fn new(id : NodeId, config: Config) -> Self {
        
        let addr =*config.listen_addrs.get(&id).unwrap(); 

        let inner = InnerNode { 
            open_connections : Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(Consensus::default())),
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
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8079);

        
        let timer = sleep(Duration::from_secs(4));
        tokio::pin!(timer);

        loop {
            tokio::select! {
                res = listener.accept() => {
                    let (stream, _) = res.unwrap();
                    let inner = self.inner.clone();
                    tokio::spawn(async move {
                        // if this returns some error, we should cancel the stream
                        if let Err(e) = inner.handle_connection(stream).await {
                            // remove connection from inner
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
                        if let Err(e) = inner.send_message(socket, message).await {
                            println!("Failed to connect to peer {}", e);
                        }
                    });
                }

            }
        }
    }
}

impl InnerNode {
    pub async fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        let mut reader = BufStream::new(&mut stream);
        loop {
            let mut buf = String::new();
            let bytes_read = reader.read_line(&mut buf).await?;
            if bytes_read == 0 {
                // connection from peer has been closed
                self.open_connections.lock().await.remove(&stream.peer_addr().unwrap());
                return Ok(())
            }
            let message: Message = serde_json::from_str(&buf)?;
            {
                //todo: Make this a separate function
                self.state.lock().await.add_to_log(message);
            }
            match message {
                Message::PrePrepareMessage(PrePrepare) => {

                }
            }
        }
    }

    pub async fn send_message(
        &self,
        peer_addr: SocketAddr,
        message: Message,
    ) -> crate::Result<()> {
        println!("Sending message");
        let mut connections = self.open_connections.lock().await;
        if let std::collections::hash_map::Entry::Vacant(e) = connections.entry(peer_addr) {
            let new_stream = BufStream::new(TcpStream::connect(peer_addr).await?);
            e.insert(new_stream);
        }

        let mut serialized_message = serde_json::to_string(&message).unwrap();
        serialized_message.push('\n');
    

        let stream = connections.get_mut(&peer_addr).unwrap();
        println!("Sending {:?}", serialized_message);
        let bytes_written = stream.get_mut().write(serialized_message.as_bytes()).await?;
        println!("{}", bytes_written);
        Ok(())
    }
}
