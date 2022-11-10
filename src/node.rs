use crate::messages::{Message, NetSenderCommand};

use std::error::Error;
use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::mpsc,
};

pub struct Node {
    /// Socket on which this node is listening for connections from peers
    pub addr: SocketAddr,
    /// Node state which will be shared across Tokio Tasks
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

    pub async fn spawn(&mut self) {
        let listener = TcpListener::bind(self.addr).await.unwrap();

        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let inner = self.inner.clone();
                tokio::spawn(async move {
                    inner.handle_connection(stream).await;
                });
            }
        }
    }
}

impl InnerNode {
    pub async fn handle_connection(&self, stream: TcpStream) {
        let mut reader = BufReader::new(stream);
        loop {
            let mut buf = String::new();
            reader.read_line(&mut buf).await.unwrap();
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
