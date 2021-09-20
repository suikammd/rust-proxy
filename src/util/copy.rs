use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::server::TlsStream;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    codec::{Packet},
    error::{ProxyError, ProxyResult},
};

pub async fn server_read_from_tcp_to_websocket<T>(
    mut tcp_stream: T,
    websocket_sink: &mut SplitSink<WebSocketStream<TlsStream<TcpStream>>, Message>,
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
        println!("STW: write to websocket successfully, total len is {}", len);
    }
}

pub async fn server_read_from_websocket_to_tcp<T>(
    mut tcp_stream: T,
    websocket_stream: &mut SplitStream<WebSocketStream<TlsStream<TcpStream>>>,
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
    println!("Test server_read_from_websocket_to_tcp done");
    Ok(())
}
