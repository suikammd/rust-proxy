use std::{
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::Poll,
};

use futures::Future;
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt, TryStreamExt,
};
use http::Request;
use log::info;
use tokio::{
    io::{AsyncRead, AsyncWrite, BufReader},
    net::TcpStream,
    time::Instant,
};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tower::Service;

use crate::{
    codec::{Addr, Packet},
    error::{ProxyError, ProxyResult},
    // util::copy::{client_read_from_tcp_to_websocket, client_read_from_websocket_to_tcp},
};

use super::Poolable;

// pub struct WebSocketStreamConnection {
//     pub raw: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
//     sink: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
//     stream: Option<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
//     last_active: Instant,
// }

pub struct WebSocketStreamConnection(pub WebSocketStream<MaybeTlsStream<TcpStream>>);

impl WebSocketStreamConnection {
    pub fn new(stream: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        Self(stream)
    }
}

// impl Drop for WebSocketStreamConnection {
//     fn drop(&mut self) {
//         if self.sink.is_none() || self.stream.is_none() {
//             info!("sink or stream is none");
//             return;
//         }

//         let sink = self.sink.take().unwrap();
//         let stream = self.stream.take().unwrap();
//         self.raw = Some(stream.reunite(sink).unwrap());
//         info!("put connect back to poll");
//     }
// }

// pub struct WebSocketStreamSink {
//     pub inner: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
//     pub raw: Weak<Mutex<WebSocketStreamConnection>>,
// }

// impl Drop for WebSocketStreamSink {
//     fn drop(&mut self) {
//         if self.inner.is_none() {
//             info!("sink is none");
//             return;
//         }
//         if let Some(raw) = self.raw.upgrade() {
//             if let Ok(mut raw) = raw.lock() {
//                 raw.sink = Some(self.inner.take().unwrap());
//             }
//         }
//         info!("put sink back successfully");
//     }
// }

// pub struct WebSocketStreamStream {
//     pub inner: Option<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
//     pub raw: Weak<Mutex<WebSocketStreamConnection>>,
// }

// impl Drop for WebSocketStreamStream {
//     fn drop(&mut self) {
//         if self.inner.is_none() {
//             info!("stream is none");
//             return;
//         }
//         if let Some(raw) = self.raw.upgrade() {
//             if let Ok(mut raw) = raw.lock() {
//                 raw.stream = Some(self.inner.take().unwrap());
//             }
//         }
//         info!("put stream back successfully");
//     }
// }

// struct WebSocketStreamConnectionFuture {
//     inner: WebSocketStreamConnection,
// }

// impl Future for WebSocketStreamConnectionFuture {
//     type Output = Option<WebSocketStreamConnection>;

//     fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         if let Some(inner) = self.inner.raw {
//             inner.send_all(Message::Ping("ping".into()));
//         }
//     }
// }

// impl Poolable for WebSocketStreamConnection {
//     type Output = WebSocketStreamConnection;
//     type Future = WebSocketStreamConnectionFuture;

//     fn checked(self) -> Self::Future {
//         WebSocketStreamConnectionFuture { inner: self }
//     }
// }

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
            // let (sink, stream) = ws_stream.split();
            Ok(WebSocketStreamConnection(ws_stream))
            // Ok(WebSocketStreamConnection {
            //     raw: Some(ws_stream),
            //     sink: None,
            //     stream: None,
            //     last_active: Instant::now(),
            // })
        })
    }
}
