use pbft::messages::{ClientRequest, Message};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufStream};
use tokio::time::sleep;
use tokio::{net::TcpListener, net::TcpStream};

use serde_json;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // note that the client only needs f + 1 replies before accepting

    let me_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38079);
    let listener = TcpListener::bind(me_addr.clone()).await.unwrap();

    let replica_addr0 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38060);
    let replica_addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38061);
    let replica_addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38062);
    let replica_addr3 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 38063);

    let addrs = vec![replica_addr0, replica_addr1, replica_addr2, replica_addr3];

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
                key: String::from("def"),
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

    let recv_fut = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move {
                        let _ = read_response(stream).await;
                    });
                }
                Err(e) => {
                    println!("{:?}", e);
                }
            }
        }
    });

    send_fut.await?;
    recv_fut.await?;

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

async fn read_response(mut stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(&mut stream);
    let mut res = String::new();
    let bytes_read = reader.read_line(&mut res).await.unwrap();
    if bytes_read == 0 {
        return Ok(());
    }
    let message: Message = serde_json::from_str(&res).unwrap();
    println!("Got: {:?}", message);
    Ok(())
}
