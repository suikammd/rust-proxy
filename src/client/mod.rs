use std::convert::{TryFrom, TryInto};
use std::sync::Arc;

use futures::{FutureExt, SinkExt, StreamExt};
use http::Request;
use log::{error, info};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::connect_async;

use crate::{
    codec::{
        Addr, Packet, {Command, RepCode},
    },
    error::{ProxyError, ProxyResult},
    util::copy::{client_read_from_tcp_to_websocket, client_read_from_websocket_to_tcp},
};

pub struct Client {
    listen_addr: String,
    server_url: Arc<String>,
    authorization: Arc<String>,
}

impl Client {
    pub fn new(
        listen_addr: String,
        proxy_addr: String,
        authorization: String,
    ) -> ProxyResult<Self> {
        if proxy_addr.is_empty() {
            return Err(ProxyError::EmptyParams);
        }
        Ok(Self {
            listen_addr,
            server_url: Arc::new(format!("wss://{}", proxy_addr)),
            authorization: Arc::new(authorization),
        })
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.listen_addr.clone()).await?;
        while let Ok((inbound, _)) = listener.accept().await {
            let serve = Client::serve(inbound, self.authorization.clone(), self.server_url.clone())
                .map(|r| {
                    if let Err(e) = r {
                        error!("Failed to transfer; error={:?}", e);
                    }
                });
            tokio::spawn(serve);
        }
        Ok(())
    }

    async fn serve(
        mut inbound: TcpStream,
        authorization: Arc<String>,
        server_url: Arc<String>,
    ) -> ProxyResult<()> {
        info!("Get new connections");

        // socks5 handshake: decide which method to use
        let (cmd, addr) = Client::socks5_handshake(&mut inbound).await?;
        info!("cmd {:?} addr {:?}", cmd, addr);
        info!("handshake successfully");

        let req = Request::builder()
            .uri(server_url.as_ref())
            .header("Authorization", authorization.as_ref())
            .body(())?;
        info!("get req {:?}", req);
        let (ws_stream, _) = connect_async(req).await?;
        info!("WebSocket handshake has been successfully completed");

        let (input_read, input_write) = inbound.split();
        let (mut output_write, output_read) = ws_stream.split();
        let input_read = BufReader::new(input_read);

        // send connect packet
        output_write.send(Packet::Connect(addr).try_into()?).await?;
        info!("send connect packet successfully");

        let (_, _) = tokio::join!(
            client_read_from_tcp_to_websocket(input_read, output_write),
            client_read_from_websocket_to_tcp(input_write, output_read)
        );

        Ok(())
    }

    async fn socks5_handshake(stream: &mut TcpStream) -> ProxyResult<(Command, Addr)> {
        let (input_read, input_write) = stream.split();
        let mut input_read = BufReader::new(input_read);
        let mut input_write = BufWriter::new(input_write);

        let mut header = vec![0u8; 2];
        input_read.read_exact(&mut header).await?;
        let socks_type = header[0];
        // validate socks type
        if socks_type != 0x05 {
            return Err(ProxyError::UnsupportedSocksType(header[0]));
        }
        let method_len = header[1];
        let mut methods = vec![0u8; method_len as usize];
        input_read.read_exact(&mut methods).await?;

        // validate methods
        // only support no auth for now
        let support = methods.into_iter().any(|method| method == 0);
        if !support {
            return Err(ProxyError::UnsupportedMethodType);
        }

        input_write.write_all(&[0x05, 0x00]).await?;
        input_write.flush().await?;
        let (cmd, addr) = Client::get_cmd_addr(input_read).await?;
        input_write.write_u8(0x05).await?;
        input_write.write_u8(RepCode::Success.into()).await?;
        input_write.write_u8(0x00).await?;
        addr.encode(input_write).await?;

        Ok((cmd, addr))
    }

    async fn get_cmd_addr<T>(mut stream: T) -> ProxyResult<(Command, Addr)>
    where
        T: AsyncRead + Unpin,
    {
        let mut header = [0u8; 3];
        stream.read_exact(&mut header).await?;
        if header[0] != 0x05 {
            return Err(ProxyError::UnsupportedSocksType(header[0]));
        }

        let cmd = Command::try_from(header[1])?;
        if cmd != Command::Connect {
            return Err(ProxyError::UnsupportedCommand);
        }

        let addr = Addr::decode(stream).await?;
        info!("addr {:?}", addr);
        Ok((cmd, addr))
    }
}
