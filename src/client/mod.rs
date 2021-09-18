use std::{
    convert::TryInto,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
};

use futures::{Future, FutureExt, Sink, SinkExt, Stream, StreamExt};

use log::{error, info};
use pin_project::pin_project;
use tokio::{
    io::{
        copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
        BufWriter, ReadBuf,
    },
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::pool::{Pool, Pooled};
use crate::{
    codec::Packet,
    pool::make_connection::{MakeWebsocketStreamConnection, WebSocketStreamConnection},
    // util::copy::{client_read_from_tcp_to_websocket, client_read_from_websocket_to_tcp},
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
        let pool: Pool<WebSocketStreamConnection> = Pool::new(10);
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
        pool: Pool<WebSocketStreamConnection>,
    ) -> ProxyResult<()> {
        info!("Get new connections");

        // socks5 handshake: decide which method to use
        let (cmd, addr) = Client::socks5_handshake(&mut inbound).await?;
        info!("cmd {:?} addr {:?}", cmd, addr);
        info!("handshake successfully");

        let mut stream = pool.get(mt).await.unwrap();
        info!("WebSocket handshake has been successfully completed");

        // let (input_read, input_write) = inbound.split();
        let mut inner = stream.inner.take().unwrap();
        // if stream.raw.is_none() {
        //     info!("fail to connect to remote server");
        //     return Err(ProxyError::Unknown(
        //         "fail to connect to remote server".into(),
        //     ));
        // }

        let (mut output_write, output_read) = inner.0.split();
        // send connect packet
        output_write.send(Packet::Connect(addr).try_into()?).await?;
        info!("send connect packet successfully");
        // let conn = Arc::new(Mutex::new(stream));
        let inner = output_write.reunite(output_read).unwrap();

        copy_bidirectional(&mut inbound, &mut WrapWebSocketStream::new(inner)).await?;
        stream.inner.insert(WebSocketStreamConnection::new(inner));

        // let sink = WebSocketStreamSink {
        //     inner: Some(output_write),
        //     raw: Arc::downgrade(&conn),
        // };
        // let stream = WebSocketStreamStream {
        //     inner: Some(output_read),
        //     raw: Arc::downgrade(&conn),
        // };
        // let input_read = BufReader::new(input_read);

        // let (_, _) = tokio::join!(
        //     client_read_from_tcp_to_websocket(input_read, sink),
        //     client_read_from_websocket_to_tcp(input_write, stream)
        // );

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

#[pin_project]
struct StreamCopy {
    #[pin]
    tcp_stream: TcpStream,
    #[pin]
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

// impl Future for StreamCopy {
//     type Output = WebSocketStream<MaybeTlsStream<TcpStream>>;

//     fn poll(
//         self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Self::Output> {
//         let mut this = self.project();
//         let mut buffer: [u8; 1024];
//         let mut read_buf =  &mut ReadBuf::new(&mut buffer);
//         match this.tcp_stream.as_mut().poll_read(cx, read_buf) {
//             Poll::Pending => {},
//             Poll::Ready(_) => {
//             },
//         }

//         // copy to websocket stream
//         if read_buf.filled().len() > 0 {
//             this.ws_stream.as_mut().
//         }
//         copy_bidirectional
//         unimplemented!()
//     }
// }

#[pin_project]
struct WrapWebSocketStream {
    #[pin]
    inner: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl WrapWebSocketStream {
    fn new(inner: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        Self { inner }
    }
}

impl AsyncRead for WrapWebSocketStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        println!("poll.....");
        let mut this = self.project();
        match this.inner.as_mut().poll_next(cx) {
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

impl AsyncWrite for WrapWebSocketStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        info!("trying to write to webscoket stream");
        let mut this = self.project();
        match this.inner.as_mut().poll_ready(cx) {
            Poll::Ready(r) => match r {
                Ok(_) => {
                    let len = buf.len() + 1;
                    let mut msg = Vec::with_capacity(len);
                    msg.push(2);
                    msg.extend(buf);
                    match this.inner.as_mut().start_send(Message::binary(msg)) {
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
                Err(e) => Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("webscoket stream poll ready fails, detail error is {:?}", e),
                ))),
            },
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        info!("trying flush websocket stream");
        let mut this = self.project();
        match this.inner.as_mut().poll_flush(cx) {
            Poll::Ready(r) => match r {
                Ok(_) => Poll::Ready(Ok(())),
                Err(e) => Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("webscoket stream poll ready fails, detail error is {:?}", e),
                ))),
            },
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let mut this = self.project();
        info!("try to shutdown");
        match this.inner.as_mut().poll_close(cx) {
            Poll::Ready(r) => match r {
                Ok(_) => Poll::Ready(Ok(())),
                Err(e) => Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("webscoket stream poll ready fails, detail error is {:?}", e),
                ))),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}
