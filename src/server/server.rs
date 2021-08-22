use std::{
    convert::{TryFrom, TryInto},
    io::{ErrorKind, Read},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    sync::Arc,
};

use futures::{FutureExt, TryFutureExt};
use tokio::{
    io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, ToSocketAddrs},
};

use crate::{
    client::Addr,
    codec::codec::{Command, RepCode},
    error::{CustomError, SocksResult},
};

pub struct Server {
    listen_addr: String,
    proxy_addr: Arc<String>,
}

impl Server {
    pub fn new(listen_addr: String, proxy_addr: String) -> Self {
        Self {
            listen_addr,
            proxy_addr: Arc::new(proxy_addr),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.listen_addr.clone()).await?;
        while let Ok((inbound, _)) = listener.accept().await {
            let serve = Server::serve(inbound, self.proxy_addr.clone()).map(|r| {
                if let Err(e) = r {
                    println!("Failed to transfer; error={}", e);
                }
            });
            tokio::spawn(serve);
        }
        Ok(())
    }

    async fn serve(mut inbound: TcpStream, proxy_addr: Arc<String>) -> Result<(), CustomError> {
        println!("get new connections from {}", inbound.peer_addr()?.ip());

        // handshake: decide which method to use
        handshake(&mut inbound).await?;
        println!("handshake successfully");
        let proxy_req = get_proxy_req(&mut inbound).await?;
        let reply = Socks5Reply::new(
            0x05,
            crate::codec::codec::RepCode::Success,
            Addr::IpV4(([127, 0, 0, 1], 8081)),
        );
        reply.encode(&mut inbound).await?;
        let mut target = TcpStream::connect(proxy_addr.as_str()).await?;
        proxy_req.encode(&mut target).await?;
        println!("connect successfully");
        match copy_bidirectional(&mut target, &mut inbound).await {
            Err(e) if e.kind() == ErrorKind::NotConnected => {
                println!("already closed");
                return Ok(());
            }
            Err(e) => {
                println!("{}", e);
                return Ok(());
            }
            Ok((s_to_t, t_to_s)) => {
                println!("{} {}", s_to_t, t_to_s);
                return Ok(());
            }
        }
    }
}

#[derive(Debug)]
pub struct Socks5Reply {
    ver: u8,
    rep: RepCode,
    addr: Addr,
}

impl Socks5Reply {
    pub fn new(ver: u8, rep: RepCode, addr: Addr) -> Self {
        Self { ver, rep, addr }
    }

    pub async fn encode(self, stream: &mut TcpStream) -> SocksResult<()> {
        stream.write_u8(self.ver).await?;
        stream.write_u8(self.rep.into()).await?;
        stream.write_u8(0x00).await?;
        self.addr.encode(stream).await?;
        Ok(())
    }
}

async fn handshake(stream: &mut TcpStream) -> Result<(), CustomError> {
    let mut header = vec![0u8; 2];

    stream.read_exact(&mut header).await?;
    let socks_type = header[0];
    // validate socks type
    if socks_type != 0x05 {
        return Err(CustomError::UnsupportedSocksType(header[0]));
    }
    let method_len = header[1];
    let mut methods = vec![0u8; method_len as usize];
    stream.read_exact(&mut methods).await?;

    // validate methods
    // only support no auth for now
    let support = methods.into_iter().any(|method| method == 0);
    if !support {
        return Err(CustomError::UnsupportedMethodType);
    }

    stream.write_all(&[0x05, 0x00]).await?;
    Ok(())
}

struct ProxyRequest {
    cmd: Command,
    addr: Addr,
}

impl ProxyRequest {
    pub async fn encode(self, stream: &mut TcpStream) -> SocksResult<()> {
        stream.write_u8(self.cmd.into()).await?;
        Ok(self.addr.encode(stream).await?)
    }
}

async fn get_proxy_req(stream: &mut TcpStream) -> SocksResult<ProxyRequest> {
    let mut header = [0u8; 3];
    stream.read_exact(&mut header).await?;
    if header[0] != 0x05 {
        return Err(CustomError::UnsupportedSocksType(header[0]));
    }

    let command = Command::try_from(header[1])?;
    if command != Command::Connect {
        return Err(CustomError::UnsupportedCommand);
    }

    let addr = Addr::decode(stream).await?;
    println!("addr {:?}", addr);
    Ok(ProxyRequest {
        cmd: command,
        addr,
    })
}
