use std::{convert::{TryFrom, TryInto}, io::{ErrorKind, Read}, net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs}};

use futures::TryFutureExt;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};

use crate::error::{Socks5Error, SocksResult};

const SOCKS5_VERSION: u8 = 0x05;

pub enum MethodType {
    NoAuth,
    UserPass,
}

impl TryFrom<u8> for MethodType {
    type Error = Socks5Error;
    fn try_from(orig: u8) -> Result<Self, Socks5Error> {
        match orig {
            0 => Ok(MethodType::NoAuth),
            2 => Ok(MethodType::UserPass),
            _ => Err(Socks5Error::UnsupportedMethodType)
        }
    }
}

#[derive(Debug)]
pub enum Addr {
    IpV4(([u8; 4], u16)),
    Domain((String, u16)),
    IpV6(([u8; 16], u16))
}

impl TryFrom<Addr> for Vec<SocketAddr> {
    type Error = Socks5Error;
    fn try_from(a: Addr) -> Result<Vec<SocketAddr>, Self::Error> {
        println!("addr is {:?}", a);
        match a {
            Addr::IpV4((addr, port)) => {
                let addr = Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]);
                Ok(vec![SocketAddrV4::new(addr, port).into()])
            },
            Addr::Domain((addr, port)) => {
                println!("addr {:?} port {:?}", addr, port);
                Ok((addr, port).to_socket_addrs()?.collect())
            },
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
        println!("addr type is {:?}",addr_type);
        match addr_type {
            1 => {
                let mut addr = [0u8; 4];
                stream.read_exact(&mut addr).await?;
                let port = stream.read_u16().await?;
                println!("addr {:?} port {:?}", addr, port);
                Ok(Addr::IpV4((addr, port)))
            },
            3 => {
                let addr_len = stream.read_u8().await?;
                let mut addr = [0u8; 255];
                stream.read(&mut addr[..(addr_len as usize)]).await?;
                let port = stream.read_u16().await?;
                Ok(Addr::Domain((std::str::from_utf8(&addr[..(addr_len as usize)]).unwrap().to_string(), port)))
            },
            4 => {
                let mut addr = [0u8; 16];
                stream.read_exact(&mut addr).await?;
                let port = stream.read_u16().await?;
                Ok(Addr::IpV6((addr, port)))
            },
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
            },
            Addr::Domain((addr, port)) => {
                stream.write(&[0x03, addr.len() as u8]).await?;
                stream.write(addr.as_bytes()).await?;
                addr_port = port;
            },
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

#[derive(Debug, PartialEq)]
enum Command {
    Connect,
    Bind,
    Udp,
}

impl TryFrom<u8> for Command {
    type Error = Socks5Error;
    fn try_from(value: u8) -> SocksResult<Self> {
        match value {
            1 => Ok(Command::Connect),
            2 => Ok(Command::Bind),
            3 => Ok(Command::Udp),
            _ => Err(Socks5Error::UnsupportedCommand)
        }
    }
}

pub enum Rep {
    Success,
    ConnectError,
    DisallowConnection,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TTLTimeout,
    UnsupportedCommand,
    UnsupportedAddrType,
    Undefined,
}

impl From<u8> for Rep {
    fn from(orig: u8) -> Self {
        match orig {
            0 => Rep::Success,
            1 => Rep::ConnectError,
            2 => Rep::DisallowConnection,
            3 => Rep::NetworkUnreachable,
            4 => Rep::HostUnreachable,
            5 => Rep::ConnectionRefused,
            6 => Rep::TTLTimeout,
            7 => Rep::UnsupportedCommand,
            8 => Rep::UnsupportedAddrType,
            _ => Rep::Undefined,
        }
    }
}

impl From<Rep> for u8 {
    fn from(orig: Rep) -> u8 {
        match orig {
            Rep::Success => 0,
            Rep::ConnectError => 1,
            Rep::DisallowConnection => 2,
            Rep::NetworkUnreachable => 3,
            Rep::HostUnreachable => 4,
            Rep::ConnectionRefused => 5,
            Rep::TTLTimeout => 6,
            Rep::UnsupportedCommand => 7,
            Rep::UnsupportedAddrType => 8,
            Rep::Undefined => 9,
        }
    }
}

pub struct Socks5Reply {
    ver: u8,
    rep: Rep,
    addr: Addr,
}

impl Socks5Reply {
    pub fn new(ver: u8, rep: Rep, addr: Addr) -> Self {
        Self {
            ver,
            rep,
            addr
        }
    }

    pub async fn encode(self, stream: &mut TcpStream) -> SocksResult<()> {
        stream.write_u8(self.ver).await?;
        stream.write_u8(self.rep.into()).await?;
        stream.write_u8(0x00).await?;
        self.addr.encode(stream).await?;
        Ok(())
    }
}

pub async fn handshake(stream: &mut TcpStream) -> Result<(), Socks5Error> {
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

    stream.write_all(&[0x05, 0x00]).await?;
    Ok(())
}

pub async fn parse_addrs(stream: &mut TcpStream) -> Result<Vec<SocketAddr>, Socks5Error> {
    let mut header = [0u8; 3];
    stream.read_exact(&mut header).await?;
    if header[0] != 0x05 {
        return Err(Socks5Error::UnsupportedSocksType(header[0]));
    }

    let command = Command::try_from(header[1])?;
    if command != Command::Connect {
        return Err(Socks5Error::UnsupportedCommand)
    }

    let addr = Addr::decode(stream).await?;
    addr.try_into()
}