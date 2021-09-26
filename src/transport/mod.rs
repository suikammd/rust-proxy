use std::{convert::TryInto, pin::Pin, task::Poll};

use futures::{Sink, Stream};
use log::{debug, error, info};
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::codec::Packet;

#[pin_project]
pub struct WebSocketConnection<T>(#[pin] pub WebSocketStream<T>);

impl<T> AsyncRead for WebSocketConnection<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project()
            .0
            .as_mut()
            .poll_next(cx)
            .map(|item| match item {
                Some(item) => match item {
                    Ok(msg) => {
                        let packet = Packet::to_packet(msg);
                        match packet {
                            Ok(packet) => match packet {
                                Packet::Connect(_) => Ok(()),
                                Packet::Data(data) => {
                                    buf.put_slice(&data);
                                    Ok(())
                                }
                                Packet::Close() => {
                                    info!("get close packet, exit copy bidirectional");
                                    Ok(())
                                }
                            },
                            Err(e) => {
                                error!("convert from message to packet error, detail is {:?}", e);
                                Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                            }
                        }
                    }
                    Err(e) => Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionAborted,
                        format!(
                            "get data from websocket connection error, detail is {:?}",
                            e
                        ),
                    )),
                },
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "websocket stream poll read fails".to_string(),
                )),
            })
    }
}

impl<T> AsyncWrite for WebSocketConnection<T>
where
    T: AsyncWrite + AsyncRead + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        debug!("trying to write to websocket stream");
        let mut this = self.project();
        this.0.as_mut().poll_ready(cx).map(|item| match item {
            Ok(_) => {
                let mut msg = Vec::with_capacity(buf.len() + 1);
                msg.push(2);
                msg.extend(buf);
                match this.0.as_mut().start_send(Message::binary(msg)) {
                    Ok(_) => {
                        debug!("write successfully, write data len is {:?}", buf.len() + 1);
                        Ok(buf.len())
                    }
                    Err(e) => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("websocket stream start send fails, detail error is {:?}", e),
                    )),
                }
            }
            Err(e) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("websocket stream poll ready fails, detail error is {:?}", e),
            )),
        })
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        debug!("trying flush websocket stream");
        self.project()
            .0
            .as_mut()
            .poll_flush(cx)
            .map(|item| match item {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!("flushing stream failed, detail error is {:?}", e);
                    Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("websocket stream poll flush fails, detail error is {:?}", e),
                    ))
                }
            })
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        // in order to keep websocket connection alive, we cannot actually close websocket connection
        // as in copy_bidirectional, when it finished copying, it will shutdown both streams
        // the close decision should up to us
        let close: Message = Packet::Close().try_into().unwrap();
        let mut this = self.project();
        match this.0.as_mut().start_send(close) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(_) => {
                println!("lalalal close");
                let _ = this.0.as_mut().poll_close(cx);
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<T> WebSocketConnection<T> {
    pub fn close(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        self.project()
            .0
            .as_mut()
            .poll_close(cx)
            .map(|item| match item {
                Ok(_) => Ok(()),
                Err(e) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("websocket stream close fails, detail error is {:?}", e),
                )),
            })
    }
}
