// extern crate ss;
use ss::server::{Server};

use tokio::io::AsyncReadExt;
use tokio::io::{copy_bidirectional, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use futures::{FutureExt, TryFutureExt};
use std::env;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listen_addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8081".to_string());
    println!("Listening on: {}", listen_addr);
    let server = Server::new(listen_addr);
    server.run().await
}


