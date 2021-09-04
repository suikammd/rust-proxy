use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use crate::{
    codec::Addr,
    error::{CustomError, SocksResult},
    util::{
        copy::{server_read_from_tcp_to_websocket, server_read_from_websocket_to_tcp},
        ssl::{load_certs, load_private_key},
    },
};
use futures::{FutureExt, StreamExt, TryStreamExt};
use rustls::NoClientAuth;
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
};
use tokio_rustls::TlsAcceptor;

pub struct Server {
    listen_addr: String,
    acceptor: TlsAcceptor,
}

impl Server {
    pub fn new(
        listen_addr: String,
        cert_pem_path: String,
        cert_key_path: String,
    ) -> SocksResult<Self> {
        let abs_cert_path = std::fs::canonicalize(PathBuf::from(cert_pem_path.as_str()))?;
        let abs_key_path = std::fs::canonicalize(PathBuf::from(cert_key_path.as_str()))?;
        let certs = load_certs(abs_cert_path)?;
        let key = load_private_key(abs_key_path)?;

        let mut server_config = rustls::ServerConfig::new(NoClientAuth::new());
        server_config.set_single_cert(certs, key)?;
        let acceptor = TlsAcceptor::from(Arc::new(server_config));
        Ok(Self {
            listen_addr,
            acceptor,
        })
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: change to websocket server
        let listener = TcpListener::bind(self.listen_addr).await?;
        while let Ok((inbound, _)) = listener.accept().await {
            let serve = serve(inbound, self.acceptor.clone()).map(|r| {
                if let Err(e) = r {
                    println!("Failed to transfer; error={}", e);
                }
            });
            tokio::spawn(serve);
        }
        Ok(())
    }
}

async fn serve(inbound: TcpStream, acceptor: TlsAcceptor) -> Result<(), CustomError> {
    // convert to tls stream
    let mut inbound = acceptor.accept(inbound).await?;
    // convert to websocket stream
    let ws_stream = tokio_tungstenite::accept_async(inbound).await?;
    // get connect addrs from connect packet
    let (mut input_write, mut input_read) = ws_stream.split();
    let addrs: Vec<SocketAddr> = match input_read.try_next().await {
        Ok(Some(msg)) => {
            let data = msg.into_data();
            Addr::from_bytes(data)?
        }
        Ok(None) => {
            return Ok(());
        }
        Err(e) => {
            // TODO
            println!("{:?}", e);
            return Ok(());
        }
    };

    let mut target = TcpStream::connect(&addrs[..]).await?;
    println!("connect to proxy addrs successfully");
    let (mut output_read, mut output_write) = target.split();
    let mut output_read = BufReader::new(output_read);
    // let mut output_write = BufWriter::new(output_write);

    let (_, _) = tokio::join!(
        server_read_from_tcp_to_websocket(output_read, input_write),
        server_read_from_websocket_to_tcp(output_write, input_read)
    );
    Ok(())
}
