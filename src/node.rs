use crate::messages::{Message, NetSenderCommand, PrePrepare};
use crate::Result;
use crate::state::State;

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
    /// Socket on which this node is listening for connections from peers
    pub addr: SocketAddr,
    /// Node state which will be shared across Tokio tasks
    pub inner: InnerNode,
}

#[derive(Clone)]
pub struct InnerNode {
    pub open_connections : Arc<Mutex<HashMap<SocketAddr, BufStream<TcpStream>>>>,

    pub state : Arc<Mutex<State>>,
}

impl Node {
    pub fn new(listen_addr: SocketAddr) -> Self {
        let inner = InnerNode { 
            open_connections : Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(State::default())),
        };
 
        Self {
            addr: listen_addr,
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
                        inner.handle_connection(stream).await;
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
                        inner.send_message(socket, message).await;
                    });
                }

            }
        }
    }
}

impl InnerNode {
    pub async fn handle_connection(&self, stream: TcpStream) -> Result<()> {
        
        let mut stream = BufStream::new(stream);
        loop {
            let mut buf = String::new();
            stream.read_line(&mut buf).await?;
            let message: Message = serde_json::from_str(&buf)?;
            {
                self.state.lock().await.add_to_log(message);
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
        if !connections.contains_key(&peer_addr) {
            let mut new_stream = BufStream::new(TcpStream::connect(peer_addr).await?);
            connections.insert(peer_addr, new_stream);
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
