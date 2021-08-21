//! A proxy that forwards data to another server and forwards that server's
//! responses back to clients.
//!
//! Because the Tokio runtime uses a thread pool, each TCP connection is
//! processed concurrently with all other TCP connections across multiple
//! threads.
//!
//! You can showcase this by running this in one terminal:
//!
//!     cargo run --example proxy
//!
//! This in another terminal
//!
//!     cargo run --example echo
//!
//! And finally this in another terminal
//!
//!     cargo run --example connect 127.0.0.1:8081
//!
//! This final terminal will connect to our proxy, which will in turn connect to
//! the echo server, and you'll be able to see data flowing between them.

#![warn(rust_2018_idioms)]
mod socks5;
mod error;

use tokio::io::{AsyncWriteExt, copy_bidirectional};
use tokio::io::{AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};

use futures::{FutureExt, TryFutureExt};
use std::env;
use std::error::Error;
use std::io::ErrorKind;

use crate::error::Socks5Error;
use crate::socks5::{Socks5Reply, Rep, Addr, handshake, parse_addrs};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listen_addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8081".to_string());

    println!("Listening on: {}", listen_addr);
    let listener = TcpListener::bind(listen_addr).await?;

    while let Ok((inbound, _)) = listener.accept().await {
        let serve = serve(inbound).map(|r| {
            if let Err(e) = r {
                println!("Failed to transfer; error={}", e);
            }
        });
        tokio::spawn(serve);
    }

    Ok(())
}

async fn serve(mut inbound: TcpStream) -> Result<(), Socks5Error> {
    println!("get new connections from {}", inbound.peer_addr()?.ip());

    // handshake: decide which method to use
    handshake(&mut inbound).await?;
    println!("handshake successfully");

    // parse addr & port
    let addrs= parse_addrs(&mut inbound).await?;
    println!("parse successfully {:?}", addrs);
    let mut target = TcpStream::connect(&addrs[..]).await?;
    println!("connect successfully");


    let reply = Socks5Reply::new(0x05, Rep::Success, Addr::IpV4(([127, 0, 0, 1], 8081)));
    reply.encode(&mut inbound).await?;
    match copy_bidirectional(&mut target, &mut inbound).await {
        Err(e) if e.kind() == ErrorKind::NotConnected => {
            println!("already closed");
            return Ok(());
        }
        Err(e) => {
            println!("{}", e);
            return Ok(());
        },
        Ok((s_to_t, t_to_s)) => {
            println!("{} {}", s_to_t, t_to_s);
            return Ok(());
        },
    }
}