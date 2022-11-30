use pbft::{NodeId, Key, Value};
use pbft::messages::{ClientRequest, Message, ClientResponse};
use pbft::node::Node;


use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufStream, Stdin};
use tokio::time::sleep;
use tokio::{net::TcpListener, net::TcpStream};
use tokio::sync::{Mutex};
use tokio::sync::mpsc::{Sender, Receiver};

use serde_json;

#[derive(Clone)]
pub struct Client {
    peer_addrs: Vec<SocketAddr>,
    listen_addr: SocketAddr,
    vote_counter: VoteCounter,
    timestamp: usize,
}

#[derive(Clone)]
pub struct VoteCounter {
    pub success_vote_quorum: Arc<Mutex<HashMap<usize, HashSet<NodeId>>>>,
    pub votes: Arc<Mutex<HashMap<(usize, usize), ClientResponse>>>,
    pub tx_client : Sender<VoteCertificate>,
    pub vote_threshold: usize,
}

#[derive(Clone)]
pub struct VoteCertificate {
    timestamp: usize,
    votes: Vec<ClientResponse>,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {

    // note that the client only needs f + 1 replies before accepting

    // the client has a -t flag.

    let me_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38079);
    let replica_addr0 = SocketAddr::from_str("127.0.0.1:38060").unwrap();
    let replica_addr1 = SocketAddr::from_str("127.0.0.1:38061").unwrap();
    let replica_addr2 = SocketAddr::from_str("127.0.0.1:38062").unwrap();
    let replica_addr3 = SocketAddr::from_str("127.0.0.1:38063").unwrap();

    let (tx_client, mut rx_client) = tokio::sync::mpsc::channel(32);

    let vote_counter = VoteCounter {
        success_vote_quorum: Arc::new(Mutex::new(HashMap::new())),
        votes: Arc::new(Mutex::new(HashMap::new())),
        tx_client, 
        vote_threshold: 1, /* number of faulty processes. We need to exceed this value */
    };


    let peer_addrs = vec![replica_addr0, replica_addr1, replica_addr2, replica_addr3];

    let outer_client = Client {
        peer_addrs,
        listen_addr: me_addr,
        vote_counter,
        timestamp: 0,
    };


    // future listening for vote count results from the client
    let vote_count_fut = tokio::spawn(async move {
        let mut succ_votes = HashMap::<usize, VoteCertificate>::new();

        loop {
            let vote_certificate = rx_client.recv().await.unwrap();
            if succ_votes.contains_key(&vote_certificate.timestamp) {continue;}
            succ_votes.insert(vote_certificate.timestamp, vote_certificate.clone());
            println!("Got enough votes for {}. VOTES: {:?}", vote_certificate.timestamp, vote_certificate.votes);
        }
    });

    // message sending logic which can be changed for new tests
    let mut client = outer_client.clone();
    let send_fut = async move {
        loop {
            client.issue_set(String::from("abc"), client.timestamp as u32).await;
            client.issue_set(String::from("abc"), client.timestamp as u32).await;
            client.issue_get(String::from("abc")).await;
            client.issue_get(String::from("def")).await;
            sleep(std::time::Duration::from_millis(2000)).await;
        }
    };

    let mut client = outer_client.clone();
    let read_cli = async move {
        let mut reader = BufReader::new(tokio::io::stdin());
        loop {
            let mut line = String::new();
            let _ = reader.read_line(&mut line).await;
            let mut args_iter = line.split_ascii_whitespace();

            let cmd = args_iter.next().unwrap();
            let key = args_iter.next().unwrap();
            if cmd.eq("set") {
                let val = args_iter.next().unwrap().parse::<u32>().unwrap();
                client.issue_set(key.to_string(), val).await;
            } else if cmd.eq("get") {
                client.issue_get(key.to_string()).await;
            }
        }
    };


    tokio::select! {
        _ = send_fut => {}
        _ = outer_client.listen() => {}
        _ = vote_count_fut => {}
        //_ = read_cli => {}
    }
    
    Ok(())
}


impl Client {

    async fn listen(&self) {
        let listener = TcpListener::bind(self.listen_addr).await.unwrap();
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let mut vote_counter = self.vote_counter.clone();
                    tokio::spawn(async move {
                        let _ = vote_counter.read_response(stream).await;
                    });
                }
                Err(e) => {
                    println!("{:?}", e);
                }
            }
        }
    }

    async fn broadcast_message(&self, message: Message) {
        for addr in self.peer_addrs.iter() {
            let node_stream = TcpStream::connect(addr).await;
            if let Ok(mut stream) = node_stream {
                let _bytes_written = stream.write(message.serialize().as_slice()).await;
            }
        }
    }

    async fn issue_set(&mut self, key: Key, value: Value) {
        let set_message: Message = Message::ClientRequestMessage(ClientRequest {
            respond_addr: self.listen_addr,
            time_stamp: self.timestamp,
            key,
            value: Some(value),
        });
        self.timestamp += 1;
        self.broadcast_message(set_message).await;
    }

    async fn issue_get(&mut self, key: Key) {
        let get_message: Message = Message::ClientRequestMessage(ClientRequest {
            respond_addr: self.listen_addr,
            time_stamp: self.timestamp,
            key,
            value: None,
        });
        self.timestamp += 1;
        self.broadcast_message(get_message).await;
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
            _ => {/* received a response which was not a client response, so just return */return Ok(());}
        };

        // if the response is not a success, then we drop it

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

                let _ = self.tx_client.send(VoteCertificate {
                    timestamp: response.time_stamp,
                    votes: succ_votes
                }).await;
            }
        }
        Ok(())
    }
}
