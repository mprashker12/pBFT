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

async fn handle_stream(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream);
    loop {
        let mut res = String::new();
        reader.read_line(&mut res).await?;
        println!("saw: {}", res);
        let res: Message = serde_json::from_str(&res).unwrap();
        println!("Got: {:?}", res);
    }
}
