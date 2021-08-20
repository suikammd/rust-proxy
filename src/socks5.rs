use std::{convert::TryFrom, io::{Error, ErrorKind}, net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs}};

use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::error::Socks5Error;

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

#[derive(Clone, Copy)]
#[derive(Debug)]
pub enum AddrType {
    V4 = 1,
    Domain = 3,
    V6 = 4,
}

impl From<u8> for AddrType {
    fn from(orig: u8) -> Self {
        match orig {
            1 => AddrType::V4,
            3 => AddrType::Domain,
            4 => AddrType::V6,
            _ => unreachable!(),
        }
    }
}

impl From<AddrType> for u8 {
    fn from(orig: AddrType) -> u8 {
        match orig {
            AddrType::V4 => 1,
            AddrType::Domain => 3,
            AddrType::V6 => 4,
        }
    }
}

#[derive(Debug)]
enum Command {
    Connect,
    Bind,
    Udp,
}

impl From<u8> for Command {
    fn from(orig: u8) -> Self {
        match orig {
            1 => Command::Connect,
            2 => Command::Bind,
            3 => Command::Udp,
            _ => unreachable!(),
        }
    }
}
enum Rep {
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
#[derive(Debug)]
pub struct Socks5Req {
    ver: u8,
    cmd: Command,
    atyp: AddrType,
    addr: Vec<u8>,
    port: u16,
    pub sockets: Vec<SocketAddr>,
}

impl Socks5Req {
    pub async fn new(stream: &mut TcpStream) -> std::io::Result<Self> {
        let mut buffer = vec![0u8; 4];
        stream.read_exact(&mut buffer).await?;
        let ver = buffer[0];
        if ver != SOCKS5_VERSION {
            return Err(Error::new(ErrorKind::Other, "oh no!"));
        }

        let cmd = Command::from(buffer[1]);
        let addr_type = AddrType::from(buffer[3]);
        let mut addr: Vec<u8> = vec![];
        match addr_type {
            AddrType::V4 => {
                println!("it is v4");
                let mut buffer = vec![0u8; 4];
                stream.read_exact(&mut buffer).await?;
                addr = buffer.clone();
            }
            AddrType::Domain => {
                println!("it is domain");
                let mut buffer = vec![0u8; 1];
                stream.read_exact(&mut buffer).await?;
                let mut buffer = vec![0u8; buffer[0] as usize];
                stream.read_exact(&mut buffer).await?;
                addr = buffer;
            }
            AddrType::V6 => {
                let mut buffer = vec![0u8; 16];
                stream.read_exact(&mut buffer).await?;
                addr = buffer;
            }
        }

        let mut buffer = vec![0u8; 2];
        stream.read_exact(&mut buffer).await?;
        let port = (buffer[0] as u16) << 8 | buffer[1] as u16;
        let sockets = to_socket_addrs(addr_type, &addr[..], port);

        let req = Socks5Req {
            ver,
            cmd,
            atyp: addr_type,
            addr,
            port,
            sockets,
        };
        println!("aaaaaaa{:?}", req);
        Ok(req)
    }
}

pub struct Socks5Reply {
    ver: u8,
    rep: Rep,
    atyp: AddrType,
    addr: Vec<u8>,
    port: u16,
}

impl Socks5Reply {
    pub fn new(req: Socks5Req) -> Self {
        Self {
            ver: req.ver,
            rep: Rep::Success,
            atyp: req.atyp,
            addr: req.addr,
            port: req.port,
        }
    }

    pub fn to_bytes(self) -> Vec<u8> {
        let mut buffer = vec![0u8; 5];
        buffer[0] = self.ver;
        buffer[1] = self.rep.into();
        buffer[2] = 0;
        buffer[3] = self.atyp.into();
        buffer.extend([self.addr.len() as u8]);
        buffer.extend(self.addr);
        let port = unsafe {
            std::mem::transmute::<u16, [u8; 2]>(self.port)
        };
        buffer.extend(port);
        buffer
    }
}

fn to_socket_addrs(atyp: AddrType, addr: &[u8], port: u16) -> Vec<SocketAddr> {
    match atyp {
        AddrType::V4 => {
            let addr = Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]);
            vec![SocketAddrV4::new(addr, port).into()]
        }
        AddrType::Domain => {
            let addr = std::str::from_utf8(addr.into()).unwrap();
            (addr, port).to_socket_addrs().unwrap().collect()
        }
        AddrType::V6 => {
            let addr: Vec<u16> = (0..8)
                .map(|x| ((addr[2 * x] as u16) << 8) | (addr[2 * x + 1] as u16))
                .collect();
            let addr = Ipv6Addr::new(
                addr[0], addr[1], addr[2], addr[3], addr[4], addr[5], addr[6], addr[7],
            );
            vec![SocketAddrV6::new(addr, port, 0, 0).into()]
        }
    }
}
