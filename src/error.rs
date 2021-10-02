#[derive(thiserror::Error, Debug)]
pub enum Error {
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
    #[error("std::io::Error `{0}`")]
    IoError(#[from] std::io::Error),
    #[error("tls error")]
    TLSError(#[from] rustls::Error),
    #[error("anyhow error")]
    AnyhowError(#[from] anyhow::Error),
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

impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::IoError(e) => e,
            _ => std::io::Error::new(std::io::ErrorKind::Other, e),
        }
    }
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
