use std::{convert::TryInto, pin::Pin, sync::Arc, task::Poll};

use futures::{SinkExt, StreamExt};
use http::{Request};
use log::info;
use tokio::{
    io::{AsyncRead, AsyncWrite, BufReader},
    net::TcpStream,
};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tower::Service;

use crate::{codec::{Addr, Packet}, error::{ProxyError, ProxyResult}, util::copy::{client_read_from_tcp_to_websocket, client_read_from_websocket_to_tcp}};

pub struct WebSocketStreamConnection(Option<WebSocketStream<MaybeTlsStream<TcpStream>>>);

impl WebSocketStreamConnection {
    pub async fn copy<R, W>(&mut self, input_read: R, input_write: W, addr: Addr) -> ProxyResult<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        if self.0.is_none() {
            info!("fail to connect to remote server");
            return Err(ProxyError::Unknown(
                "fail to connect to remote server".into(),
            ));
        }
        let (mut output_write, output_read) = self.0.take().unwrap().split();
        let input_read = BufReader::new(input_read);

        // send connect packet
        output_write.send(Packet::Connect(addr).try_into()?).await?;
        info!("send connect packet successfully");

        let (output_write, output_read) = tokio::join!(
            client_read_from_tcp_to_websocket(input_read, output_write),
            client_read_from_websocket_to_tcp(input_write, output_read)
        );

        match (output_write, output_read) {
            (Ok(output_write), Ok(output_read)) => {
                self.0.insert(output_read.reunite(output_write).unwrap());
            }
            (_, _) => (),
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MakeWebsocketStreamConnection {
    pub server_url: Arc<String>,
    pub authorization: Arc<String>,
}

impl MakeWebsocketStreamConnection {
    pub fn new(server_url: String, authorization: String) -> Self {
        Self {
            server_url: Arc::new(server_url),
            authorization: Arc::new(authorization),
        }
    }
}

impl<T> Service<T> for MakeWebsocketStreamConnection {
    type Response = WebSocketStreamConnection;

    type Error = tokio_tungstenite::tungstenite::Error;

    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: T) -> Self::Future {
        let req = Request::builder()
            .uri(self.server_url.as_ref())
            .header("Authorization", self.authorization.as_ref())
            .body(())
            .unwrap();
        Box::pin(async {
            let (ws_stream, _) = connect_async(req).await?;
            Ok(WebSocketStreamConnection(Some(ws_stream)))
        })
    }
}
