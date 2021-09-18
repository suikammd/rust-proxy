use rustls::TLSError;
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("parse url error")]
    UrlParseError(#[from] url::ParseError),
    // socks5 error
    // start
    #[error("socks type `{0}` not supported")]
    UnsupportedSocksType(u8),
    #[error("method type not supported")]
    UnsupportedMethodType,
    #[error("addr type not supported")]
    UnsupportedAddrType,
    #[error("command not supported")]
    UnsupportedCommand,
    #[error("invalid rep code")]
    InvalidRepCode,
    // end
    #[error("invalid packet type")]
    InvalidPacketType,
    #[error("packet is not binary message")]
    PacketNotBinaryMessage,
    #[error("build client http request error")]
    HttpError(#[from] http::Error),
    #[error("empty params")]
    EmptyParams,
    // cert start
    #[error("invalid private key")]
    InvalidPrivateKey,
    #[error("invalid cert")]
    InvalidCert,
    // end
    #[error("invalid server status, (expected {expected:?}, found {found:?})")]
    InvalidServerStatus { expected: String, found: String },
    #[error("data store disconnected `{0}`")]
    Disconnect(#[from] io::Error),
    #[error("tls error")]
    TLSError(#[from] TLSError),
    #[error("tungstenite error")]
    TungsteniteError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("reunite read/write stream error")]
    ReuniteError,
    #[error("the data for key `{0}` is not available")]
    Redaction(String),
    #[error("invalid header (expected {expected:?}, found {found:?})")]
    InvalidHeader { expected: String, found: String },
    #[error("unknown data store error, detail is `{0}`")]
    Unknown(String),
}

pub type ProxyResult<T> = Result<T, ProxyError>;
