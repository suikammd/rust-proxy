use std::{convert::TryInto, sync::Arc};

use futures::{FutureExt};

use futures::SinkExt;
use log::{error, info};
use tokio::{
    io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{tungstenite::Message};

use crate::pool::Pool;
use crate::transport::WebSocketConnection;
use crate::{
    codec::Packet,
    pool::make_connection::{MakeWebsocketStreamConnection, WebSocketOutboundConnection},
};
use crate::{
    codec::{
        Addr, {Command, RepCode},
    },
    error::{ProxyError, ProxyResult},
};

pub struct Client {
    listen_addr: String,
    mt: MakeWebsocketStreamConnection,
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
            mt: MakeWebsocketStreamConnection {
                server_url: Arc::new(format!("wss://{}", proxy_addr)),
                authorization: Arc::new(authorization),
            },
        })
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(self.listen_addr.clone()).await?;
        let pool: Pool<WebSocketOutboundConnection> = Pool::new(10);
        while let Ok((inbound, _)) = listener.accept().await {
            let serve = Client::serve(inbound, self.mt.clone(), pool.clone()).map(|r| {
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
        mt: MakeWebsocketStreamConnection,
        pool: Pool<WebSocketOutboundConnection>,
    ) -> ProxyResult<()> {
        info!("Get new connections");

        // socks5 handshake: decide which method to use
        let (cmd, addr) = Client::socks5_handshake(&mut inbound).await?;
        info!("cmd {:?} addr {:?}", cmd, addr);
        info!("handshake successfully");

        let mut stream = pool.get(mt).await?;
        info!("WebSocket handshake has been successfully completed");

        let outbound = stream.inner.take().unwrap();
        let mut outbound = WebSocketConnection(outbound.0);
        let addr_msg: Message = Packet::Connect(addr).try_into()?;
        outbound.0.send(addr_msg).await?;
        info!("send connect packet successfully");
        let (a_to_b, b_to_a) = copy_bidirectional(&mut outbound, &mut inbound).await?;
        info!("finished copy data a_to_b {} b_to_a {}", a_to_b, b_to_a);
        let _ = stream.inner.insert(WebSocketOutboundConnection(outbound.0));
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
        let cmd = Command::decode(&mut input_read).await?;
        let addr = Addr::decode(&mut input_read).await?;
        input_write.write_u8(0x05).await?;
        input_write.write_u8(RepCode::Success.into()).await?;
        input_write.write_u8(0x00).await?;
        addr.encode(input_write).await?;

        Ok((cmd, addr))
    }
}
