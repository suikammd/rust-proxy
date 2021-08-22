// extern crate ss;
use ss::server::{Server};

use tokio::io::AsyncReadExt;
use tokio::io::{copy_bidirectional, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use futures::{FutureExt, TryFutureExt};
use std::env;



use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "opt")]
struct ServerOpt {
    #[structopt(default_value = "127.0.0.1:8081", long)]
    listen_addr: String,
    #[structopt(default_value = "127.0.0.1:8080", long)]
    proxy_addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = ServerOpt::from_args();
    println!("input opts are {:?}", opt);
    println!("Listening on: {}", opt.listen_addr);
    let server = Server::new(opt.listen_addr, opt.proxy_addr);
    server.run().await
}

