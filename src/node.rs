use std::str::FromStr;
use std::{collections::HashMap, net::SocketAddr};
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr};

use crate::{net_sender::NetSender, messages::{NetSenderCommand, Message, PrePrepare}};
use tokio::sync::mpsc;


pub struct Node {
    pub tx_net_sender : mpsc::Sender<NetSenderCommand>,
}


impl Node {

    pub async fn spawn(&mut self) {
        let message = Message::PrePrepareMessage(PrePrepare {
            view: 5,
            seq_num: 6,
            digest: 7,
        });
        let message2 = Message::PrePrepareMessage(PrePrepare {
            view: 5,
            seq_num: 6,
            digest: 100,
        });
        let peer_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8079);
        self.send_message(peer_addr, message).await;
        self.send_message(peer_addr, message2).await;
    }

    pub async fn send_message(&self, peer_addr: SocketAddr, message : Message) -> Result<(), Box<dyn Error>> {
        let command = NetSenderCommand::Send { peer_addr, message };
        self.tx_net_sender.send(command).await?;
        Ok(())
    }

}