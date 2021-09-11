use std::{convert::TryInto, net::SocketAddr, path::PathBuf, sync::Arc};

use crate::{
    codec::Packet,
    error::{ProxyError, ProxyResult},
    util::{
        copy::{server_read_from_tcp_to_websocket, server_read_from_websocket_to_tcp},
        ssl::{load_certs, load_private_key},
    },
};
use futures::{FutureExt, StreamExt, TryStreamExt};

use log::{error, info};
use rustls::NoClientAuth;
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
};
use tokio_rustls::TlsAcceptor;

pub struct Server {
    listen_addr: String,
    acceptor: TlsAcceptor,
    authorization: Arc<String>,
}

impl Server {
    pub fn new(
        listen_addr: String,
        cert_pem_path: String,
        cert_key_path: String,
        authorization: String,
    ) -> ProxyResult<Self> {
        if listen_addr.is_empty()
            || cert_pem_path.is_empty()
            || cert_key_path.is_empty()
            || authorization.is_empty()
        {
            return Err(ProxyError::EmptyParams);
        }
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
            authorization: Arc::new(authorization),
        })
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: change to websocket server
        let listener = TcpListener::bind(self.listen_addr).await?;
        while let Ok((inbound, _)) = listener.accept().await {
            let serve =
                serve(inbound, self.authorization.clone(), self.acceptor.clone()).map(|r| {
                    if let Err(e) = r {
                        error!("Failed to transfer; error={:?}", e);
                    }
                });
            tokio::spawn(serve);
        }
        Ok(())
    }
}

async fn serve(
    inbound: TcpStream,
    authorization: Arc<String>,
    acceptor: TlsAcceptor,
) -> ProxyResult<()> {
    info!("get new connections");
    // convert to tls stream
    let inbound = acceptor.accept(inbound).await?;
    // convert to websocket stream
    // let ws_stream = tokio_tungstenite::accept_async(inbound).await?;
    let ws_stream = tokio_tungstenite::accept_hdr_async(
        inbound,
        |req: &http::Request<()>,
         res: http::Response<()>|
         -> Result<http::Response<()>, http::Response<Option<String>>> {
            if req.headers().get("Authorization").map(|x| x.as_bytes())
                != Some(authorization.as_bytes())
            {
                info!("incorrect auth");
                return Err(http::Response::new(Some(
                    "invalid authorization".to_string(),
                )));
            }
            info!("correct auth");
            Ok(res)
        },
    )
    .await?;
    info!("build websocket stream successfully");
    // get connect addrs from connect packet
    let (mut input_write, mut input_read) = ws_stream.split();
    loop {
        let addrs: Vec<SocketAddr> = match input_read.try_next().await {
            Ok(Some(msg)) => match Packet::to_packet(msg) {
                Ok(Packet::Connect(addr)) => addr.try_into()?,
                Ok(_) => {
                    info!("packet is not binary message");
                    continue
                },
                Err(e) => return Err(ProxyError::Unknown(format!("{:?}", e))),
            },
            Ok(None) => {
                info!("get none packet");
                continue
            }
            Err(e) => {
                return Err(ProxyError::Unknown(format!("{:?}", e)))
            }
        };
        let mut target = TcpStream::connect(&addrs[..]).await?;
        info!("connect to proxy addrs successfully");
        let (output_read, output_write) = target.split();
        let output_read = BufReader::new(output_read);

        tokio::select!(
            _ = server_read_from_tcp_to_websocket(output_read, &mut input_write) => {
                info!("server read from tcp to websocket finished");
            }
            _ = server_read_from_websocket_to_tcp(output_write, &mut input_read) => {
                info!("server read from websocket to tcp finished");
            }
        );
    }
}
