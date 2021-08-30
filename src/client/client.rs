use std::{
    convert::{TryFrom, TryInto},
    io::{ErrorKind, Read},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    sync::Arc,
};

use bytes::{BufMut, BytesMut};
use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};
use tokio::{
    io::{self, copy_bidirectional, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, ToSocketAddrs},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    codec::codec::{Command, RepCode},
    error::{CustomError, SocksResult},
    server::Addr,
    util::copy::{client_read_from_tcp_to_websocket, client_read_from_websocket_to_tcp},
};

pub struct Client {
    listen_addr: String,
    proxy_addr: Arc<String>,
}

impl Client {
    pub fn new(listen_addr: String, proxy_addr: String) -> Self {
        Self {
            listen_addr,
            proxy_addr: Arc::new(format!("ws://{}", proxy_addr)),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.listen_addr.clone()).await?;
        while let Ok((inbound, _)) = listener.accept().await {
            let serve = Client::serve(inbound, self.proxy_addr.clone()).map(|r| {
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

        // socks5 handshake: decide which method to use
        handshake_with_browser(&mut inbound).await?;
        println!("handshake successfully");
        let proxy_req = get_proxy_req(&mut inbound).await?;
        let reply = Socks5Reply::new(
            0x05,
            crate::codec::codec::RepCode::Success,
            Addr::IpV4(([127, 0, 0, 1], 8081)),
        );
        reply.encode(&mut inbound).await?;

        // TODO: change to secure websocket connection
        let url = url::Url::parse(proxy_addr.as_str()).unwrap();
        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
        println!("WebSocket handshake has been successfully completed");

        let (input_read, input_write) = inbound.split();
        let (mut output_write, output_read) = ws_stream.split();

        // send connect packet
        let mut bytes = BytesMut::new();
        proxy_req.to_bytes(&mut bytes);
        println!("connect packet {:?}", bytes);
        output_write.send(Message::binary(bytes.to_vec())).await?;
        println!("send connect packet successfully");

        let (_, _) = tokio::join!(
            client_read_from_tcp_to_websocket(input_read, output_write),
            client_read_from_websocket_to_tcp(input_write, output_read)
        );

        Ok(())
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

async fn handshake_with_browser(stream: &mut TcpStream) -> Result<(), CustomError> {
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

    pub fn to_bytes(&self, bytes: &mut BytesMut) {
        self.addr.to_bytes(&self.cmd, bytes);
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
    Ok(ProxyRequest { cmd: command, addr })
}
