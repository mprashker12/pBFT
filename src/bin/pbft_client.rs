use pbft::messages::Message;
use tokio::{net::TcpListener, stream, io::AsyncReadExt};
use tokio::io::{BufReader, AsyncBufReadExt};

use serde_json;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut listener = TcpListener::bind("127.0.0.1:8079").await.unwrap();
    match listener.accept().await {
        Ok((mut stream, _)) => {
            let mut reader = BufReader::new(stream);
            
            let mut res = String::new();
            reader.read_line(&mut res).await?;
            println!("saw: {}", res);
            let res : Message = serde_json::from_str(&res).unwrap();
            println!("Got: {:?}", res);


            let mut res = String::new();
            reader.read_line(&mut res).await?;
            println!("saw: {}", res);
            let res : Message = serde_json::from_str(&res).unwrap();
            println!("Got: {:?}", res);
        }
        Err(e) => {return Err(e);}
    }
    Ok(())
}