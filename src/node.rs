use crate::messages::{Message, NetSenderCommand, PrePrepare};

use std::error::Error;
use std::net::SocketAddr;

use tokio::io::{BufStream};
use tokio::net::{TcpListener, TcpStream};
use tokio::{
    io::{AsyncBufReadExt},
    sync::mpsc,
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
    pub tx_net_sender: mpsc::Sender<NetSenderCommand>,
}

impl Node {
    pub fn new(listen_addr: SocketAddr, tx: mpsc::Sender<NetSenderCommand>) -> Self {
        let inner = InnerNode { tx_net_sender: tx };
 
        Self {
            addr: listen_addr,
            inner,
        }
    }

    pub async fn run(&mut self) {
        let listener = TcpListener::bind(self.addr).await.unwrap();

        let message = Message::PrePrepareMessage(PrePrepare {
            view: 7,
            seq_num: 8,
            digest: 9,
        });
        let timer = sleep(Duration::from_secs(4));
        tokio::pin!(timer);

        use std::net::{IpAddr, Ipv4Addr};
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8079);
        let inner2 = self.inner.clone();
        tokio::spawn(async move {
            inner2.send_message(socket, message).await;
        });

        loop {
            tokio::select! {
                res = listener.accept() => {
                    let (stream, _) = res.unwrap();
                    let inner = self.inner.clone();
                    tokio::spawn(async move {
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
    pub async fn handle_connection(&self, stream: TcpStream) {
        
        let mut stream = BufStream::new(stream);
        loop {
            let mut buf = String::new();
            stream.read_line(&mut buf).await.unwrap();
            
        }
    }

    pub async fn send_message(
        &self,
        peer_addr: SocketAddr,
        message: Message,
    ) -> Result<(), Box<dyn Error>> {
        let command = NetSenderCommand::Send { peer_addr, message };
        self.tx_net_sender.send(command).await?;
        Ok(())
    }
}
