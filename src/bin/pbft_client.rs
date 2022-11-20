use pbft::messages::{ClientRequest, Message};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufStream};
use tokio::{net::TcpListener, net::TcpStream};

use serde_json;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // note that the client only needs f + 1 replies before accepting

    let me_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8079);
    let listener = TcpListener::bind(me_addr.clone()).await.unwrap();
    
    let mut node_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8060);

    let send_fut = tokio::spawn(async move {
        let mut node_stream = TcpStream::connect(node_addr).await.unwrap();
        let message: Message = Message::ClientRequestMessage(ClientRequest {
            respond_addr: me_addr,
            time_stamp: 0,
            key: String::from("def"),
            value: Some(3),
        });
        let _bytes_written = node_stream.write(message.serialize().as_slice()).await;

        let mut node_stream = TcpStream::connect(node_addr).await.unwrap();
        let message: Message = Message::ClientRequestMessage(ClientRequest {
            respond_addr: me_addr,
            time_stamp: 1,
            key: String::from("def"),
            value: Some(4),
        });
        let _bytes_written = node_stream.write(message.serialize().as_slice()).await;

        let mut node_stream = TcpStream::connect(node_addr).await.unwrap();
        let message: Message = Message::ClientRequestMessage(ClientRequest {
            respond_addr: me_addr,
            time_stamp: 2,
            key: String::from("def"),
            value: None,
        });
        let _bytes_written = node_stream.write(message.serialize().as_slice()).await;
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

async fn read_response(mut stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(&mut stream);
    let mut res = String::new();
    let bytes_read = reader.read_line(&mut res).await.unwrap();
    if bytes_read == 0 {return Ok(())}
    let message: Message = serde_json::from_str(&res).unwrap();
    println!("Got: {:?}", message);
    Ok(())
}
