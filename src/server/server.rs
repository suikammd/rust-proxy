use std::{
    convert::{TryFrom, TryInto},
    io::{ErrorKind, Read},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs},
};

use futures::{FutureExt, TryFutureExt};
use tokio::{io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional}, net::{TcpListener, TcpStream}};

use crate::{codec::codec::{Command, RepCode}, error::{Socks5Error, SocksResult}};

pub struct Server {
    listen_addr: String,
}

impl Server {
    pub fn new(listen_addr: String) -> Self {
        Self {
            listen_addr
        }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.listen_addr).await?;
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
}

async fn serve(mut inbound: TcpStream) -> Result<(), Socks5Error> {
    println!("get new connections from {}", inbound.peer_addr()?.ip());

    // handshake: decide which method to use
    handshake(&mut inbound).await?;
    println!("handshake successfully");

    // parse addr & port
    let addrs = parse_addrs(&mut inbound).await?;
    println!("parse successfully {:?}", addrs);
    let mut target = TcpStream::connect(&addrs[..]).await?;
    println!("connect successfully");

    let reply = Socks5Reply::new(
        0x05,
        crate::codec::codec::RepCode::Success,
        Addr::IpV4(([127, 0, 0, 1], 8081)),
    );
    reply.encode(&mut inbound).await?;
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

#[derive(Debug)]
pub enum Addr {
    IpV4(([u8; 4], u16)),
    Domain((String, u16)),
    IpV6(([u8; 16], u16)),
}

impl TryFrom<Addr> for Vec<SocketAddr> {
    type Error = crate::error::Socks5Error;
    fn try_from(a: Addr) -> Result<Vec<SocketAddr>, Self::Error> {
        println!("addr is {:?}", a);
        match a {
            Addr::IpV4((addr, port)) => {
                let addr = Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]);
                Ok(vec![SocketAddrV4::new(addr, port).into()])
            }
            Addr::Domain((addr, port)) => {
                println!("addr {:?} port {:?}", addr, port);
                Ok((addr, port).to_socket_addrs()?.collect())
            }
            Addr::IpV6((addr, port)) => {
                let addr: Vec<u16> = (0..8)
                    .map(|x| ((addr[2 * x] as u16) << 8) | (addr[2 * x + 1] as u16))
                    .collect();
                let addr = Ipv6Addr::new(
                    addr[0], addr[1], addr[2], addr[3], addr[4], addr[5], addr[6], addr[7],
                );
                Ok(vec![SocketAddrV6::new(addr, port, 0, 0).into()])
            }
        }
    }
}

impl Addr {
    async fn decode(stream: &mut TcpStream) -> SocksResult<Self> {
        let addr_type = stream.read_u8().await?;
        println!("addr type is {:?}", addr_type);
        match addr_type {
            1 => {
                let mut addr = [0u8; 4];
                stream.read_exact(&mut addr).await?;
                let port = stream.read_u16().await?;
                println!("addr {:?} port {:?}", addr, port);
                Ok(Addr::IpV4((addr, port)))
            }
            3 => {
                let addr_len = stream.read_u8().await?;
                let mut addr = [0u8; 255];
                stream.read(&mut addr[..(addr_len as usize)]).await?;
                let port = stream.read_u16().await?;
                Ok(Addr::Domain((
                    std::str::from_utf8(&addr[..(addr_len as usize)])
                        .unwrap()
                        .to_string(),
                    port,
                )))
            }
            4 => {
                let mut addr = [0u8; 16];
                stream.read_exact(&mut addr).await?;
                let port = stream.read_u16().await?;
                Ok(Addr::IpV6((addr, port)))
            }
            _ => Err(Socks5Error::UnsupportedAddrType),
        }
    }

    async fn encode(self, stream: &mut TcpStream) -> SocksResult<()> {
        let addr_port;
        match self {
            Addr::IpV4((addr, port)) => {
                stream.write(&[0x01]).await?;
                stream.write(&addr).await?;
                addr_port = port;
            }
            Addr::Domain((addr, port)) => {
                stream.write(&[0x03, addr.len() as u8]).await?;
                stream.write(addr.as_bytes()).await?;
                addr_port = port;
            }
            Addr::IpV6((addr, port)) => {
                stream.write(&[0x04]).await?;
                stream.write(&addr).await?;
                addr_port = port;
            }
        }
        stream.write_u16(addr_port).await?;
        Ok(())
    }
}

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
    let support = methods.into_iter().any(|method| method == 0);
    if !support {
        return Err(Socks5Error::UnsupportedMethodType);
    }

    stream.write_all(&[0x05, 0x00]).await?;
    Ok(())
}

async fn parse_addrs(stream: &mut TcpStream) -> Result<Vec<SocketAddr>, Socks5Error> {
    let mut header = [0u8; 3];
    stream.read_exact(&mut header).await?;
    if header[0] != 0x05 {
        return Err(Socks5Error::UnsupportedSocksType(header[0]));
    }

    let command = Command::try_from(header[1])?;
    if command != Command::Connect {
        return Err(Socks5Error::UnsupportedCommand);
    }

    let addr = Addr::decode(stream).await?;
    addr.try_into()
}
