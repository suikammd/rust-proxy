use std::{convert::TryInto, pin::Pin, sync::Arc, task::Poll};

use futures::{Sink, SinkExt, Stream, StreamExt};
use http::Request;
use log::info;
use pin_project::pin_project;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf},
    net::TcpStream,
};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tower::Service;

use crate::{
    codec::{Addr, Packet},
    error::{ProxyError, ProxyResult},
    util::copy::{client_read_from_tcp_to_websocket, client_read_from_websocket_to_tcp},
};

// pub struct WebSocketStreamConnection(Option<WebSocketStream<MaybeTlsStream<TcpStream>>>);
#[pin_project]
pub struct WebSocketStreamConnection(#[pin] pub WebSocketStream<MaybeTlsStream<TcpStream>>);

// impl WebSocketStreamConnection {
//     pub async fn copy<R, W>(&mut self, input_read: R, input_write: W, addr: Addr) -> ProxyResult<()>
//     where
//         R: AsyncRead + Unpin,
//         W: AsyncWrite + Unpin,
//     {
//         if self.0.is_none() {
//             info!("fail to connect to remote server");
//             return Err(ProxyError::Unknown(
//                 "fail to connect to remote server".into(),
//             ));
//         }
//         let (mut output_write, output_read) = self.0.take().unwrap().split();
//         let input_read = BufReader::new(input_read);

//         // send connect packet
//         output_write.send(Packet::Connect(addr).try_into()?).await?;
//         info!("send connect packet successfully");

//         let (output_write, output_read) = tokio::join!(
//             client_read_from_tcp_to_websocket(input_read, output_write),
//             client_read_from_websocket_to_tcp(input_write, output_read)
//         );

//         match (output_write, output_read) {
//             (Ok(output_write), Ok(output_read)) => {
//                 self.0.insert(output_read.reunite(output_write).unwrap());
//             }
//             (_, _) => (),
//         }
//         Ok(())
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
            // Ok(WebSocketStreamConnection(Some(ws_stream)))
            Ok(WebSocketStreamConnection(ws_stream))
        })
    }
}

impl AsyncRead for WebSocketStreamConnection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        println!("poll.....");
        let mut this = self.project();
        match this.0.as_mut().poll_next(cx) {
            Poll::Pending => {
                println!("pending.....");
                Poll::Pending
            }
            Poll::Ready(item) => {
                println!("trying to read data...");
                match item {
                    Some(item) => match item {
                        Ok(msg) => match msg {
                            tokio_tungstenite::tungstenite::Message::Binary(data) => {
                                buf.put_slice(&data);
                                Poll::Ready(Ok(()))
                            }
                            _ => Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("expected binary not {:?}", msg),
                            ))),
                        },
                        Err(e) => Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::ConnectionAborted,
                            format!(
                                "get data from websocket connection error, detail is {:?}",
                                e
                            ),
                        ))),
                    },
                    None => Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "expected binary, but get no data".to_string(),
                    ))),
                }
            }
        }
    }
}

impl AsyncWrite for WebSocketStreamConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        info!("trying to write to webscoket stream");
        let mut this = self.project();
        match this.0.as_mut().poll_ready(cx) {
            Poll::Ready(r) => match r {
                Ok(_) => {
                    println!("W POLL READY OK");
                    let len = buf.len() + 1;
                    let mut msg = Vec::with_capacity(len);
                    msg.push(2);
                    msg.extend(buf);
                    match this.0.as_mut().start_send(Message::binary(msg)) {
                        Ok(_) => {
                            println!("write successfully, write data len is {:?}", len);
                            Poll::Ready(Ok(len))
                        }
                        Err(e) => Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("webscoket stream poll ready fails, detail error is {:?}", e),
                        ))),
                    }
                }
                Err(e) => {
                    println!("W POLL READY ERR");
                    Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("webscoket stream poll ready fails, detail error is {:?}", e),
                    )))
                }
            },
            Poll::Pending => {
                println!("W POLL READY PENDING");
                Poll::Pending
            }
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        info!("trying flush websocket stream");
        let mut this = self.project();
        match this.0.as_mut().poll_flush(cx) {
            Poll::Ready(r) => match r {
                Ok(_) => {println!("F POLL READY OK");Poll::Ready(Ok(()))},
                Err(e) => {println!("F POLL READY ERR");Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("webscoket stream poll ready fails, detail error is {:?}", e),
                )))},
            },
            Poll::Pending => {println!("F POLL READY PENDING");Poll::Pending},
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let mut this = self.project();
        info!("try to shutdown");
        match this.0.as_mut().poll_close(cx) {
            Poll::Ready(r) => match r {
                Ok(_) => {
                    println!("S POLL READY OK");
                    Poll::Ready(Ok(()))
                },
                Err(e) => {println!("S POLL READY ERR");Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("webscoket stream poll ready fails, detail error is {:?}", e),
                )))},
            },
            Poll::Pending => {println!("S POLL READY PENDING"); Poll::Pending},
        }
    }
}
