use pbft::NodeId;
use pbft::messages::{ClientRequest, Message, ClientResponse};
use pbft::node::Node;


use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufStream};
use tokio::time::sleep;
use tokio::{net::TcpListener, net::TcpStream};
use tokio::sync::{Mutex};
use tokio::sync::mpsc::{Sender, Receiver};

use serde_json;

#[derive(Clone)]
pub struct VoteCounter {
    pub success_vote_quorum: Arc<Mutex<HashMap<usize, HashSet<NodeId>>>>,
    pub votes: Arc<Mutex<HashMap<(usize, usize), ClientResponse>>>,
    pub tx_client : Sender<VoteReceit>,
    pub vote_threshold: usize,
}

#[derive(Clone)]
pub struct VoteReceit {
    timestamp: usize,
    num_votes: usize,
    votes: Vec<ClientResponse>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // note that the client only needs f + 1 replies before accepting

    let me_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38079);
    let listener = TcpListener::bind(me_addr.clone()).await.unwrap();

    let replica_addr0 = SocketAddr::from_str("127.0.0.1:38060").unwrap();

    //let replica_addr0 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38060);
    let replica_addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38061);
    let replica_addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38062);
    let replica_addr3 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38063);

    let addrs = vec![replica_addr0, replica_addr1, replica_addr2, replica_addr3];

    
    // message sending logic which can be changed for new tests
    let send_fut = tokio::spawn(async move {
        let mut timestamp: u32 = 0;
        loop {
            timestamp += 1;
            let message: Message = Message::ClientRequestMessage(ClientRequest {
                respond_addr: me_addr,
                time_stamp: timestamp as usize,
                key: String::from("def"),
                value: Some(timestamp),
            });
            broadcast_message(&addrs, message).await;

            timestamp += 1;
            let message: Message = Message::ClientRequestMessage(ClientRequest {
                respond_addr: me_addr,
                time_stamp: timestamp as usize,
                key: String::from("abc"),
                value: Some(timestamp),
            });
            broadcast_message(&addrs, message).await;

            timestamp += 1;
            let message: Message = Message::ClientRequestMessage(ClientRequest {
                respond_addr: me_addr,
                time_stamp: timestamp as usize,
                key: String::from("abc"),
                value: None,
            });
            broadcast_message(&addrs, message).await;

            timestamp += 1;
            let message: Message = Message::ClientRequestMessage(ClientRequest {
                respond_addr: me_addr,
                time_stamp: timestamp as usize,
                key: String::from("def"),
                value: None,
            });
            broadcast_message(&addrs, message).await;

            sleep(std::time::Duration::from_secs(4)).await;
        }
    });



    let (tx_client, mut rx_client) = tokio::sync::mpsc::channel(32);

    let vote_counter = VoteCounter {
        success_vote_quorum: Arc::new(Mutex::new(HashMap::new())),
        votes: Arc::new(Mutex::new(HashMap::new())),
        tx_client, 
        vote_threshold: 1,
    };

    let recv_fut = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let mut vote_counter = vote_counter.clone();
                    tokio::spawn(async move {
                        let _ = vote_counter.read_response(stream).await;
                    });
                }
                Err(e) => {
                    println!("{:?}", e);
                }
            }
        }
    });

    let vote_count_fut = tokio::spawn(async move {

        let mut succ_votes = HashMap::<usize, VoteReceit>::new();

        loop {
            let vote_receit = rx_client.recv().await.unwrap();
            if succ_votes.contains_key(&vote_receit.timestamp) {continue;}
            succ_votes.insert(vote_receit.timestamp, vote_receit.clone());
            println!("Got enough votes for {}. VOTES: {:?}", vote_receit.timestamp, vote_receit.votes);
        }
    });

    send_fut.await?;
    recv_fut.await?;
    vote_count_fut.await?;

    Ok(())
}

async fn broadcast_message(addrs: &[SocketAddr], message: Message) {
    for addr in addrs.iter() {
        let node_stream = TcpStream::connect(addr).await;
        if let Ok(mut stream) = node_stream {
            let _bytes_written = stream.write(message.serialize().as_slice()).await;
        }
    }
}
impl VoteCounter {
    async fn read_response(&mut self, mut stream: TcpStream) -> std::io::Result<()> {
        let mut reader = BufReader::new(&mut stream);
        let mut res = String::new();
        let bytes_read = reader.read_line(&mut res).await.unwrap();
        if bytes_read == 0 {
            return Ok(());
        }
        let response: Message = serde_json::from_str(&res).unwrap();
        let response = match response {
            Message::ClientResponseMessage(response) => {response}
            _ => {return Ok(());}
        };

        //println!("Got: {:?}", &response);

        if response.success {
            let mut success_vote_quorum = self.success_vote_quorum.lock().await;
            let mut votes = self.votes.lock().await;

            if success_vote_quorum.get_mut(&response.time_stamp).is_none() {
                success_vote_quorum.insert(response.time_stamp, HashSet::<NodeId>::new());
            }
            
            votes.insert((response.time_stamp, response.id), response.clone());
            let curr_quorum = success_vote_quorum.get_mut(&response.time_stamp).unwrap();
            curr_quorum.insert(response.id);
            if curr_quorum.len() > self.vote_threshold {
                // send message alerting enough votes
                let mut succ_votes = Vec::<ClientResponse>::new();
                for id in curr_quorum.iter() {
                    succ_votes.push(votes.get(&(response.time_stamp, *id)).unwrap().clone());
                }

                let _ = self.tx_client.send(VoteReceit {
                    timestamp: response.time_stamp,
                    num_votes: succ_votes.len(),
                    votes: succ_votes
                }).await;
            }
        }
        Ok(())
    }
}
