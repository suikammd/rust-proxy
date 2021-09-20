use std::convert::TryInto;

use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use http::Response;
use log::info;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::TcpStream,
};
use tokio_rustls::server::TlsStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{
    codec::{Addr, Packet},
    error::{ProxyError, ProxyResult},
    pool::Pooled,
};

pub struct StreamCopyClient(pub Pooled<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response<()>)>);

impl StreamCopyClient {
    pub async fn copy<R, W>(&mut self, input_read: R, input_write: W, addr: Addr) -> ProxyResult<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let (mut output_write, output_read) = self.0.inner.take().unwrap().0.split();
        let input_read = BufReader::new(input_read);

        // send connect packet
        output_write.send(Packet::Connect(addr).try_into()?).await?;
        info!("send connect packet successfully");

        let (output_write, output_read) = tokio::join!(
            client_read_from_tcp_to_websocket(input_read, output_write),
            client_read_from_websocket_to_tcp(input_write, output_read)
        );

        match (output_read, output_write) {
            (Ok(output_read), Ok(output_write)) => {
                let reunite_stream = output_read.reunite(output_write);
                match reunite_stream {
                    Ok(reunite_stream) => {
                        self.0.inner.insert((reunite_stream, Response::new(())));
                        Ok(())
                    }
                    Err(_) => Err(ProxyError::ReuniteError),
                }
            }
            (_, _) => Err(ProxyError::Unknown(
                "read/write stream can not reunite together, may be one of the copy fail".into(),
            )),
        }
    }
}

pub async fn client_read_from_tcp_to_websocket<T>(
    mut tcp_stream: T,
    mut websocket_sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
) -> ProxyResult<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>
where
    T: AsyncRead + Unpin,
{
    loop {
        let mut buffer = vec![0; 1024];
        let len = tcp_stream.read(&mut buffer).await?;
        if len == 0 {
            info!("no data from tcp stream, send close packet to server");
            websocket_sink.send(Packet::Close().try_into()?).await?;
            return Ok(websocket_sink);
        }

        unsafe {
            buffer.set_len(len);
        }
        websocket_sink
            .send(Packet::Data(buffer).try_into()?)
            .await?;
    }
}

pub async fn server_read_from_tcp_to_websocket<T>(
    mut tcp_stream: T,
    // websocket_sink: &mut SplitSink<WebSocketStream<TlsStream<TcpStream>>, Message>,
    websocket_sink: &mut SplitSink<WebSocketStream<TcpStream>, Message>,
) -> ProxyResult<()>
where
    T: AsyncRead + Unpin,
{
    loop {
        let mut buffer = vec![0; 1024];
        let len = tcp_stream.read(&mut buffer).await?;
        if len == 0 {
            return Ok(());
        }

        unsafe {
            buffer.set_len(len);
        }
        websocket_sink.send(Message::binary(buffer)).await?;
    }
}

pub async fn client_read_from_websocket_to_tcp<T>(
    mut tcp_stream: T,
    mut websocket_stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
) -> ProxyResult<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>
where
    T: AsyncWrite + Unpin,
{
    while let Some(msg) = websocket_stream.next().await {
        let msg = msg?.into_data();
        tcp_stream.write_all(&msg).await?;
    }
    Ok(websocket_stream)
}

pub async fn server_read_from_websocket_to_tcp<T>(
    mut tcp_stream: T,
    // websocket_stream: &mut SplitStream<WebSocketStream<TlsStream<TcpStream>>>,
    websocket_stream: &mut SplitStream<WebSocketStream<TcpStream>>,
) -> ProxyResult<()>
where
    T: AsyncWrite + Unpin,
{
    while let Some(msg) = websocket_stream.next().await {
        match Packet::to_packet(msg?)? {
            Packet::Connect(_) => {
                return Err(ProxyError::Unknown(
                    "unexpected packet type `Connect`".into(),
                ))
            }
            Packet::Data(data) => tcp_stream.write_all(&data).await?,
            Packet::Close() => return Ok(()),
        }
    }
    Ok(())
}
