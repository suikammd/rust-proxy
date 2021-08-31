use futures::{SinkExt, StreamExt, stream::{SplitSink, SplitStream}};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpStream, tcp::{ReadHalf, WriteHalf}}};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

use crate::error::CustomError;

pub async fn client_read_from_tcp_to_websocket<'a>(mut tcp_stream: ReadHalf<'a>, mut websocket_sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>) -> Result<(), CustomError> {
    loop {
        let mut buffer = vec![0; 1024];
        let len = tcp_stream.read(&mut buffer).await?;
        if len == 0 {
            return Ok(());
        }

        unsafe {
            buffer.set_len(len);
        }
        // println!("CTW {:?} len is {:?}", buffer, len);
        websocket_sink.send(Message::binary(buffer)).await?;
    }
}

pub async fn client_read_from_websocket_to_tcp<'a>(mut tcp_stream: WriteHalf<'a>, mut websocket_stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>) -> Result<(), CustomError> {
    loop {
        match websocket_stream.next().await {
            Some(msg) => {
                let msg = msg?.into_data();
                // println!("CWT {:?}", msg);
                tcp_stream.write_all(&msg).await?;
            },
            None => break
        }
    }
    Ok(())
}

pub async fn server_read_from_tcp_to_websocket<'a>(mut tcp_stream: ReadHalf<'a>, mut websocket_sink: SplitSink<WebSocketStream<TcpStream>, Message>) -> Result<(), CustomError> {
    loop {
        let mut buffer = vec![0; 1024];
        let len = tcp_stream.read(&mut buffer).await?;
        // println!("STW {:?}", buffer);
        if len == 0 {
            return Ok(());
        }

        unsafe {
            buffer.set_len(len);
        }
        websocket_sink.send(Message::binary(buffer)).await?;
    }
}

pub async fn server_read_from_websocket_to_tcp<'a>(mut tcp_stream: WriteHalf<'a>, mut websocket_stream: SplitStream<WebSocketStream<TcpStream>>) -> Result<(), CustomError> {
    loop {
        match websocket_stream.next().await {
            Some(msg) => {
                let msg = msg?.into_data();
                // println!("SWT {:?}", msg);
                tcp_stream.write_all(&msg).await?;
            },
            None => break
        }
    }
    Ok(())
}