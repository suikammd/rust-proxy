use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CustomError {
    #[error("parse server url error")]
    UrlParseError(#[from] url::ParseError),
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
    #[error("invalid packet type")]
    InvalidPacketType,
    #[error("invalid domain")]
    InvalidDomain,
    #[error("data store disconnected `{0}`")]
    Disconnect(#[from] io::Error),
    #[error("tungstenite error")]
    TungsteniteError(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("the data for key `{0}` is not available")]
    Redaction(String),
    #[error("invalid header (expected {expected:?}, found {found:?})")]
    InvalidHeader { expected: String, found: String },
    #[error("unknown data store error")]
    Unknown,
}

pub type SocksResult<T> = Result<T, CustomError>;
