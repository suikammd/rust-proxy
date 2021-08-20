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
use crate::socks5::{Socks5Reply, Socks5Req};

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
    // first handshake
    handshake(&mut inbound).await?;
    println!("handshake successfully");
    let req = Socks5Req::new(&mut inbound).await?;
    let mut target = TcpStream::connect(&(req.sockets)[..]).await?;
    println!("connect successfully");
    let bytes = Socks5Reply::new(req).to_bytes();
    inbound.write(bytes.as_slice()).await?;
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

async fn handshake(stream: &mut TcpStream) -> Result<(), Socks5Error> {
    let mut header = vec![0u8; 2];
    stream.read_exact(&mut header).await?;
    let socks_type = header[0];
    // validate socks type
    if socks_type != 0x05 {
        return Err(Socks5Error::UnsupportedSocksType(header[0]));
    }
    let method_len = header[1];
    let mut methods = vec![0u8; method_len as usize];
    stream.read_exact(&mut methods).await?;

    // validate methods
    // only support no auth for now
    let support = methods.into_iter().any(|method|method == 0);
    if !support {
        return Err(Socks5Error::UnsupportedMethodType);
    }

    stream.write(&[0x05, 0x00]).await?;
    Ok(())
}