use std::{
    fs::{File},
    io::{BufReader},
    path::PathBuf,
};


use rustls::internal::pemfile::certs;

use crate::error::{ProxyError, ProxyResult};

pub fn load_certs(filename: PathBuf) -> ProxyResult<Vec<rustls::Certificate>> {
    let certs = certs(&mut BufReader::new(File::open(filename)?));
    if let Ok(certs) = certs {
        return Ok(certs)
    }
    Err(ProxyError::InvalidCert)
}

pub fn load_private_key(filename: PathBuf) -> ProxyResult<rustls::PrivateKey> {
    let mut reader = &mut BufReader::new(File::open(filename)?);

    loop {
        match rustls_pemfile::read_one(&mut reader)? {
            Some(rustls_pemfile::Item::RSAKey(key)) => return Ok(rustls::PrivateKey(key)),
            Some(rustls_pemfile::Item::PKCS8Key(key)) => return Ok(rustls::PrivateKey(key)),
            None => break,
            _ => {}
        }
    }
    Err(ProxyError::InvalidPrivateKey)
}
