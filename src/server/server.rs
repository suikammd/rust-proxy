use std::{convert::{TryFrom, TryInto}, io::ErrorKind, net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs}};

use crate::{
    codec::codec::Command,
    error::{CustomError, SocksResult},
};
use futures::{FutureExt};
use tokio::{io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional}, net::{TcpListener, TcpStream}};

pub struct Server {
    listen_addr: String,
}

impl Server {
    pub fn new(listen_addr: String) -> Self {
        Self {
            listen_addr,
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

async fn serve(mut inbound: TcpStream) -> Result<(), CustomError> {
    println!("get new connections from {}", inbound.peer_addr()?.ip());

    let addrs = parse_addrs(&mut inbound).await?;
    let mut target = TcpStream::connect(&addrs[..]).await?;
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

async fn parse_addrs(stream: &mut TcpStream) -> Result<Vec<SocketAddr>, CustomError> {
    let command = Command::try_from(stream.read_u8().await?)?;
    if command != Command::Connect {
        return Err(CustomError::UnsupportedCommand);
    }

    let addr = Addr::decode(stream).await?;
    addr.try_into()
}

#[derive(Debug)]
pub enum Addr {
    IpV4(([u8; 4], u16)),
    Domain((String, u16)),
    IpV6(([u8; 16], u16)),
}

impl TryFrom<Addr> for Vec<SocketAddr> {
    type Error = crate::error::CustomError;
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
    pub async fn decode(stream: &mut TcpStream) -> SocksResult<Self> {
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
            _ => Err(CustomError::UnsupportedAddrType),
        }
    }

    pub async fn encode(self, stream: &mut TcpStream) -> SocksResult<()> {
        let addr_port;
        match self {
            Addr::IpV4((addr, port)) => {
                stream.write_u8(0x01).await?;
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
