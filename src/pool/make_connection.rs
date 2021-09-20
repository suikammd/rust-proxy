use std::{pin::Pin, sync::Arc, task::Poll};

use http::Request;
use pin_project::pin_project;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tower::Service;

#[pin_project]
pub struct WebSocketOutboundConnection(#[pin] pub WebSocketStream<MaybeTlsStream<TcpStream>>);

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
    type Response = WebSocketOutboundConnection;

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
            // Ok(WebSocketStreamConnection(Some(ws_stream)))
            Ok(WebSocketOutboundConnection(ws_stream))
        })
    }
}
