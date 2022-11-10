use crate::messages::{Message, NetSenderCommand};

use std::collections::HashMap;
use std::net::SocketAddr;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;

use serde_json;

pub struct NetSender {
    /// Receiving end of channel for Messages to send to the network from the nodes
    pub command_receiver: Receiver<NetSenderCommand>,
    /// Cache of connections to peers which are already opened
    pub open_connections: HashMap<SocketAddr, TcpStream>,
}

impl NetSender {
    pub async fn spawn(&mut self) {
        println!("Spawning NetSender");
        loop {
            let command = self.command_receiver.recv().await;
            if command.is_none() {
                println!("Internal Channel Closed. Net-Sender breaking;");
                break;
            }
            let command = command.unwrap();
            self.process_command(command).await;
        }
    }

    pub async fn process_command(&mut self, command: NetSenderCommand) {
        match command {
            NetSenderCommand::Send { peer_addr, message } => {
                match self.send_message(peer_addr, message).await {
                    Ok(_) => {}
                    Err(_) => {
                        self.open_connections.remove(&peer_addr);
                    }
                }
            }
        }
    }

    pub async fn send_message(
        &mut self,
        peer_addr: SocketAddr,
        message: Message,
    ) -> std::io::Result<usize> {
        let mut serialized_message = serde_json::to_string(&message).unwrap();
        serialized_message.push('\n');

        if let Some(stream) = self.open_connections.get_mut(&peer_addr) {
            println!("Cache hit in connections");
            return stream.write(serialized_message.as_bytes()).await;
        }

        match TcpStream::connect(peer_addr).await {
            Ok(mut stream) => {
                let res = stream.write(serialized_message.as_bytes()).await;
                self.open_connections.insert(peer_addr, stream);
                res
            }
            Err(e) => {
                println!("Failed to connect to peer");
                Err(e)
            }
        }
    }
}
