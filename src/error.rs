use thiserror::Error;
use std::io;


#[derive(Error, Debug)]
pub enum Socks5Error {
    #[error("socks type `{0}` not supported")]
    UnsupportedSocksType(u8),
    #[error("method type not supported")]
    UnsupportedMethodType,
    #[error("addr type not supported")]
    UnsupportedAddrType,
    #[error("command not supported")]
    UnsupportedCommand,
    #[error("invalid domain")]
    InvalidDomain,
    #[error("data store disconnected")]
    Disconnect(#[from] io::Error),
    #[error("the data for key `{0}` is not available")]
    Redaction(String),
    #[error("invalid header (expected {expected:?}, found {found:?})")]
    InvalidHeader {
        expected: String,
        found: String,
    },
    #[error("unknown data store error")]
    Unknown,
}

pub type SocksResult<T> = Result<T, Socks5Error>;