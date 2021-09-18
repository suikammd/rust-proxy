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

use crate::{codec::{Addr, Packet}, error::{ProxyError, ProxyResult}, pool::{Pooled}};

// pub async fn client_read_from_tcp_to_websocket<T>(
//     mut tcp_stream: T,
//     mut websocket_sink: WebSocketStreamSink,
// ) -> ProxyResult<WebSocketStreamSink>
// where
//     T: AsyncRead + Unpin,
// {
//     let mut inner = websocket_sink.inner.take().unwrap();
//     loop {
//         let mut buffer = vec![0; 1024];
//         let len = tcp_stream.read(&mut buffer).await?;
//         if len == 0 {
//             info!("no data from tcp stream, send close packet to server");
//             inner.send(Packet::Close().try_into()?).await?;
//             websocket_sink.inner.insert(inner);
//             return Ok(websocket_sink);
//         }

//         unsafe {
//             buffer.set_len(len);
//         }
//         inner.send(Packet::Data(buffer).try_into()?)
//             .await?;
//     }
// }

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
            println!("read no data");
            return Ok(());
        }

        unsafe {
            buffer.set_len(len);
        }
        println!("STW data: {:?} len: {:?}", buffer, len);
        websocket_sink.send(Message::binary(buffer)).await?;
        println!("send successfully")
    }
}

// pub async fn client_read_from_websocket_to_tcp<T>(
//     mut tcp_stream: T,
//     mut websocket_stream: WebSocketStreamStream,
// ) -> ProxyResult<WebSocketStreamStream>
// where
//     T: AsyncWrite + Unpin,
// {
//     let mut inner = websocket_stream.inner.take().unwrap();
//     while let Some(msg) = inner.next().await {
//         let msg = msg?.into_data();
//         tcp_stream.write_all(&msg).await?;
//     }
//     info!("prepare to drop stream");
//     websocket_stream.inner.insert(inner);
//     Ok(websocket_stream)
// }from_websocket_to_tcp<T>(
//     mut tcp_stream: T,
//     mut websocket_stream: WebSocketStreamStream,
// ) -> ProxyResult<WebSocketStreamStream>
// where
//     T: AsyncWrite + Unpin,
// {
//     let mut inner = websocket_stream.inner.take().unwrap();
//     while let Some(msg) = inner.next().await {
//         let msg = msg?.into_data();
//         tcp_stream.write_all(&msg).await?;
//     }
//     info!("prepare to drop stream");
//     websocket_stream.inner.insert(inner);
//     Ok(websocket_stream)
// }

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
            Packet::Data(data) => {
                println!("SWT data: {:?} len: {:?}", data, data.len());
                tcp_stream.write_all(&data).await?
            },
            Packet::Close() => return Ok(()),
        }
    }
    Ok(())
}
