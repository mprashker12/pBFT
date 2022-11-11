use pbft::messages::Message;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::{net::TcpListener, net::TcpStream};

use serde_json;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8079").await.unwrap();
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(async move {
                    handle_stream(stream).await;
                });
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
}

async fn handle_stream(mut stream: TcpStream) -> std::io::Result<()> {
    println!("Handling stream {:?}", stream.peer_addr());
    let mut reader = BufReader::new(&mut stream);
    loop {
        let mut res = String::new();
        let bytes_read = reader.read_line(&mut res).await?;
        if bytes_read == 0 {
            println!("Breaking connection;"); return Ok(());
        }
        let message: Message = serde_json::from_str(&res).unwrap();
        println!("Got: {:?}", message);
    }
}
