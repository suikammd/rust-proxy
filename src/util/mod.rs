use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use std::{fs::File, io::BufReader, path::PathBuf};

use crate::error::Error;

pub fn load_certs(filename: PathBuf) -> std::io::Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(&filename)?);
    let certs = certs(&mut reader).collect::<Result<Vec<_>, _>>()?;
    if certs.is_empty() {
        return Err(Error::InvalidCert.into());
    }
    Ok(certs)
}

pub fn load_private_key(filename: PathBuf) -> std::io::Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(File::open(&filename)?);

    // Try PKCS8 first
    let mut keys = pkcs8_private_keys(&mut reader).collect::<Result<Vec<_>, _>>()?;
    if !keys.is_empty() {
        return Ok(rustls::pki_types::PrivateKeyDer::Pkcs8(keys.remove(0)));
    }

    // Fallback to RSA
    let mut reader = BufReader::new(File::open(&filename)?);
    let mut keys = rsa_private_keys(&mut reader).collect::<Result<Vec<_>, _>>()?;
    if !keys.is_empty() {
        return Ok(rustls::pki_types::PrivateKeyDer::Pkcs1(keys.remove(0)));
    }

    Err(Error::InvalidPrivateKey.into())
}
