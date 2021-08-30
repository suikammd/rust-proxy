use std::{
    convert::{TryFrom, TryInto},
    io::ErrorKind,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs},
};

use crate::{codec::codec::Command, error::{CustomError, SocksResult}, util::copy::{client_read_from_tcp_to_websocket, client_read_from_websocket_to_tcp, server_read_from_tcp_to_websocket, server_read_from_websocket_to_tcp}};
use bytes::{BufMut, BytesMut};
use futures::{FutureExt, StreamExt, TryStreamExt};
use futures_util::future;
use tokio::{
    io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpSocket, TcpStream},
};
use tokio_tungstenite::tungstenite::Message;

pub struct Server {
    listen_addr: String,
}

impl Server {
    pub fn new(listen_addr: String) -> Self {
        Self { listen_addr }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: change to websocket server
        let listener = TcpListener::bind(self.listen_addr).await?;
        while let Ok((inbound, addr)) = listener.accept().await {
            let serve = serve(inbound, addr).map(|r| {
                if let Err(e) = r {
                    println!("Failed to transfer; error={}", e);
                }
            });
            tokio::spawn(serve);
        }
        Ok(())
    }
}

async fn serve(inbound: TcpStream, addr: SocketAddr) -> Result<(), CustomError> {
    // parse connect packet
    let ws_stream = tokio_tungstenite::accept_async(inbound)
        .await
        .expect("Error during the websocket handshake occurred");

    // get connect addrs from connect packet
    let (mut input_write, mut input_read) = ws_stream.split();
    let addrs: Vec<SocketAddr>;
    match input_read.try_next().await {
        Ok(Some(msg)) => {
            let data = msg.into_data();
            addrs = Addr::from_bytes(data)?;
        }
        Ok(None) => {
            return Ok(());
        }
        Err(e) => {
            // TODO
            println!("{:?}", e);
            return Ok(());
        }
    }

    let mut target = TcpStream::connect(&addrs[..]).await?;
    println!("connect to proxy addrs successfully");
    let (mut output_read, mut output_write) = target.split();

    let (_, _) = tokio::join!(
        server_read_from_tcp_to_websocket(output_read, input_write),
        server_read_from_websocket_to_tcp(output_write, input_read)
    );
    Ok(())
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

    pub fn to_bytes(&self, command: &Command, bytes: &mut BytesMut) {
        let cmd = match command {
            Command::Connect => 1,
            Command::Bind => 2,
            Command::Udp => 3,
        };
        println!("addr is {:?}", self);

        match self {
            Addr::IpV4(addr) => {
                bytes.put_u8(cmd << 4 | 1);
                bytes.put_u16(addr.1);
                bytes.put_slice(&addr.0[..]);
            }
            Addr::Domain(addr) => {
                bytes.put_u8(cmd << 4 | 3);
                bytes.put_u16(addr.1);
                bytes.put(addr.0.as_bytes());
            }
            Addr::IpV6(addr) => {
                bytes.put_u8(cmd << 4 | 4);
                bytes.put_u16(addr.1);
                bytes.put_slice(&addr.0[..]);
            }
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Vec<SocketAddr>, CustomError> {
        println!("conenct packes {:?}", bytes);
        let addr_type = bytes[0] & 0x0f;
        let cmd = bytes[0] >> 4;

        if cmd != 1 {
            return Err(CustomError::UnsupportedCommand);
        }

        let port = unsafe {
            std::mem::transmute::<[u8; 2], u16>([bytes[2], bytes[1]])
        };

        let addr: Addr;
        match addr_type {
            1 => {
                assert!(bytes.len() == 7);
                addr = Addr::IpV4(([bytes[3], bytes[4], bytes[5], bytes[6]], port));
            },
            3 => {
                let domain = String::from_utf8_lossy(&bytes[3..]);
                addr = Addr::Domain((domain.into(), port));
            },
            4 => {
                assert!(bytes.len() == 19);
                addr = Addr::IpV6(([bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18]], port));
            }
            _ => unreachable!()
        }
        println!("addr is {:?}", addr);
        addr.try_into()
    }
}
