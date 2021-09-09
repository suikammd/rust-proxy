use std::sync::Arc;
use std::task::Poll;
use std::{convert::TryInto, pin::Pin};

use futures::{FutureExt, SinkExt, StreamExt};
use http::{Request, Response};
use log::{error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tower::Service;

use crate::pool::Pool;
use crate::{
    codec::{
        Addr, Packet, {Command, RepCode},
    },
    error::{ProxyError, ProxyResult},
    util::copy::{client_read_from_tcp_to_websocket, client_read_from_websocket_to_tcp},
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
        let pool: Pool<MakeWebsocketStreamResponse> = Pool::new(10);
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
        pool: Pool<MakeWebsocketStreamResponse>,
    ) -> ProxyResult<()> {
        info!("Get new connections");

        // socks5 handshake: decide which method to use
        let (cmd, addr) = Client::socks5_handshake(&mut inbound).await?;
        info!("cmd {:?} addr {:?}", cmd, addr);
        info!("handshake successfully");

        let mut stream = pool.get(mt).await.unwrap();
        info!("WebSocket handshake has been successfully completed");

        let (input_read, input_write) = inbound.split();
        let (ws_stream, http_response) = stream.inner.take().unwrap();
        let (mut output_write, output_read) = ws_stream.split();
        let input_read = BufReader::new(input_read);

        // send connect packet
        output_write.send(Packet::Connect(addr).try_into()?).await?;
        info!("send connect packet successfully");

        let (output_write, output_read) = tokio::join!(
            client_read_from_tcp_to_websocket(input_read, output_write),
            client_read_from_websocket_to_tcp(input_write, output_read)
        );
        stream
            .inner
            .insert((output_read?.reunite(output_write?).unwrap(), http_response));

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

type MakeWebsocketStreamResponse = (WebSocketStream<MaybeTlsStream<TcpStream>>, Response<()>);
#[derive(Debug, Clone)]
struct MakeWebsocketStreamConnection {
    server_url: Arc<String>,
    authorization: Arc<String>,
}

impl<T> Service<T> for MakeWebsocketStreamConnection {
    type Response = MakeWebsocketStreamResponse;

    type Error = tokio_tungstenite::tungstenite::Error;

    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: T) -> Self::Future {
        let req = Request::builder()
            .uri(self.server_url.as_ref())
            .header("Authorization", self.authorization.as_ref())
            .body(())
            .unwrap();
        Box::pin(connect_async(req))
    }
}
