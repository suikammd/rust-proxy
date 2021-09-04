use std::{
    fs::{File},
    io::{self, BufReader, Result},
    path::PathBuf,
};

use rustls::internal::pemfile::certs;

pub fn load_certs(filename: PathBuf) -> Result<Vec<rustls::Certificate>> {
    certs(&mut BufReader::new(File::open(filename)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
}

pub fn load_private_key(filename: PathBuf) -> Result<rustls::PrivateKey> {
    let mut reader = &mut BufReader::new(File::open(filename)?);

    loop {
        match rustls_pemfile::read_one(&mut reader).expect("cannot parse private key .pem file") {
            Some(rustls_pemfile::Item::RSAKey(key)) => return Ok(rustls::PrivateKey(key)),
            Some(rustls_pemfile::Item::PKCS8Key(key)) => return Ok(rustls::PrivateKey(key)),
            None => break,
            _ => {}
        }
    }
    Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))
}
