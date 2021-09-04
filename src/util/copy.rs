use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{WriteHalf}, TcpStream,
    },
};
use tokio_rustls::server::TlsStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::error::CustomError;

pub async fn client_read_from_tcp_to_websocket<T>(
    mut tcp_stream: T,
    mut websocket_sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
) -> Result<(), CustomError>
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

pub async fn server_read_from_tcp_to_websocket<T>(
    mut tcp_stream: T,
    mut websocket_sink: SplitSink<WebSocketStream<TlsStream<TcpStream>>, Message>,
) -> Result<(), CustomError>
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

pub async fn client_read_from_websocket_to_tcp(
    mut tcp_stream: WriteHalf<'_>,
    mut websocket_stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
) -> Result<(), CustomError> {
    while let Some(msg) = websocket_stream.next().await {
        let msg = msg?.into_data();
        tcp_stream.write_all(&msg).await?;
    }
    Ok(())
}

pub async fn server_read_from_websocket_to_tcp(
    mut tcp_stream: WriteHalf<'_>,
    mut websocket_stream: SplitStream<WebSocketStream<TlsStream<TcpStream>>>,
) -> Result<(), CustomError> {
    while let Some(msg) = websocket_stream.next().await {
        let msg = msg?.into_data();
        tcp_stream.write_all(&msg).await?;
    }
    Ok(())
}
