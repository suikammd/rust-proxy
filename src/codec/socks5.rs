use std::{convert::{TryFrom, TryInto}, io::Cursor, net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs}};

use byteorder::{LittleEndian};
use bytes::{BufMut, BytesMut};
use log::info;
use tokio::{io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter}, net::TcpStream};

use crate::error::{ProxyError, ProxyResult};

pub enum MethodType {
    NoAuth,
    UserPass,
}

impl TryFrom<u8> for MethodType {
    type Error = ProxyError;
    fn try_from(orig: u8) -> ProxyResult<Self> {
        match orig {
            0 => Ok(MethodType::NoAuth),
            2 => Ok(MethodType::UserPass),
            _ => Err(ProxyError::UnsupportedMethodType),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Connect,
    Bind,
    Udp,
}

impl TryFrom<u8> for Command {
    type Error = ProxyError;
    fn try_from(value: u8) -> ProxyResult<Self> {
        match value {
            1 => Ok(Command::Connect),
            2 => Ok(Command::Bind),
            3 => Ok(Command::Udp),
            _ => Err(ProxyError::UnsupportedCommand),
        }
    }
}

impl TryFrom<Command> for u8 {
    type Error = ProxyError;
    fn try_from(command: Command) -> ProxyResult<u8> {
        match command {
            Command::Connect => Ok(1),
            Command::Bind => Ok(2),
            Command::Udp => Ok(3),
        }
    }
}

#[derive(Debug)]
pub enum RepCode {
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

impl TryFrom<u8> for RepCode {
    type Error = ProxyError;
    fn try_from(orig: u8) -> Result<Self, Self::Error> {
        match orig {
            0 => Ok(RepCode::Success),
            1 => Ok(RepCode::ConnectError),
            2 => Ok(RepCode::DisallowConnection),
            3 => Ok(RepCode::NetworkUnreachable),
            4 => Ok(RepCode::HostUnreachable),
            5 => Ok(RepCode::ConnectionRefused),
            6 => Ok(RepCode::TTLTimeout),
            7 => Ok(RepCode::UnsupportedCommand),
            8 => Ok(RepCode::UnsupportedAddrType),
            _ => Err(ProxyError::InvalidRepCode),
        }
    }
}

impl From<RepCode> for u8 {
    fn from(orig: RepCode) -> u8 {
        match orig {
            RepCode::Success => 0,
            RepCode::ConnectError => 1,
            RepCode::DisallowConnection => 2,
            RepCode::NetworkUnreachable => 3,
            RepCode::HostUnreachable => 4,
            RepCode::ConnectionRefused => 5,
            RepCode::TTLTimeout => 6,
            RepCode::UnsupportedCommand => 7,
            RepCode::UnsupportedAddrType => 8,
            RepCode::Undefined => 9,
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
    type Error = crate::error::ProxyError;
    fn try_from(a: Addr) -> Result<Vec<SocketAddr>, Self::Error> {
        info!("addr is {:?}", a);
        match a {
            Addr::IpV4((addr, port)) => {
                let addr = Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]);
                Ok(vec![SocketAddrV4::new(addr, port).into()])
            }
            Addr::Domain((addr, port)) => Ok((addr, port).to_socket_addrs()?.collect()),
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
    pub async fn decode<T>(mut stream: T) -> ProxyResult<Self>
    where
        T: AsyncRead + Unpin,
    {
        let addr_type = stream.read_u8().await?;
        info!("addr type is {:?}", addr_type);
        match addr_type {
            1 => {
                let mut addr = [0u8; 4];
                stream.read_exact(&mut addr).await?;
                let port = stream.read_u16().await?;
                info!("addr {:?} port {:?}", addr, port);
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
            _ => Err(ProxyError::UnsupportedAddrType),
        }
    }

    pub async fn encode<T>(&self, mut stream: BufWriter<T>) -> ProxyResult<()>
    where
        T: AsyncWrite + Unpin,
    {
        let addr_port;
        match self {
            Addr::IpV4((addr, port)) => {
                stream.write_u8(0x01).await?;
                stream.write(addr).await?;
                addr_port = *port;
            }
            Addr::Domain((addr, port)) => {
                stream.write(&[0x03, addr.len() as u8]).await?;
                stream.write(addr.as_bytes()).await?;
                addr_port = *port;
            }
            Addr::IpV6((addr, port)) => {
                stream.write(&[0x04]).await?;
                stream.write(addr).await?;
                addr_port = *port;
            }
        }
        stream.write_u16(addr_port).await?;
        stream.flush().await?;
        Ok(())
    }

    pub fn to_bytes(&self, bytes: &mut BytesMut) {
        match self {
            Addr::IpV4(addr) => {
                bytes.put_u8(1);
                bytes.put_u16(addr.1);
                bytes.put_slice(&addr.0[..]);
            }
            Addr::Domain(addr) => {
                bytes.put_u8(3);
                bytes.put_u16(addr.1);
                bytes.put(addr.0.as_bytes());
            }
            Addr::IpV6(addr) => {
                bytes.put_u8(4);
                bytes.put_u16(addr.1);
                bytes.put_slice(&addr.0[..]);
            }
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> ProxyResult<Addr> {
        // info!("connect packets {:?}", bytes);
        // let addr_type = bytes[0] & 0x0f;
        // let cmd = bytes[0] >> 4;

        // if cmd != 1 {
        //     return Err(ProxyError::UnsupportedCommand);
        // }

        info!("{:?}", bytes);
        let mut cursor = Cursor::new(bytes);

        let addr_type = byteorder::ReadBytesExt::read_u8(&mut cursor)?;
        let port = byteorder::ReadBytesExt::read_u16::<LittleEndian>(&mut cursor)?;

        let addr: Addr;
        match addr_type {
            1 => {
                assert!(bytes.len() == 7);
                let mut ipv4 = [0; 4];
                std::io::Read::read_exact(&mut cursor, &mut ipv4)?;
                addr = Addr::IpV4((ipv4, port));
            }
            3 => {
                let domain = String::from_utf8_lossy(&bytes[cursor.position() as _..]);
                addr = Addr::Domain((domain.into(), port));
            }
            4 => {
                assert!(bytes.len() == 19);
                let mut ipv6 = [0; 16];
                std::io::Read::read_exact(&mut cursor, &mut ipv6)?;
                addr = Addr::IpV6((ipv6, port));
            }
            _ => unreachable!(),
        }
        info!("addr is {:?}", addr);
        Ok(addr)
    }

    pub async fn parse_addrs(stream: &mut TcpStream) -> Result<Vec<SocketAddr>, ProxyError> {
        let command = Command::try_from(stream.read_u8().await?)?;
        if command != Command::Connect {
            return Err(ProxyError::UnsupportedCommand);
        }

        let addr = Addr::decode(stream).await?;
        addr.try_into()
    }
}
