use crate::Error;
use std::{
    fmt,
    net::{IpAddr, SocketAddr},
};

const QR_KEY: [u8; 7] = [0x74, 0xb2, 0x92, 0x5b, 0x4a, 0x21, 0xda];
const AUTH_KEY: [u8; 7] = [0xb6, 0xd5, 0x92, 0xc4, 0xa7, 0x83, 0xe1];

/// In-memory credentials. Parsing performs no network access or DNS lookup.
#[derive(Clone)]
pub struct Pairing {
    pub(crate) endpoints: Vec<SocketAddr>,
    pub(crate) password: String,
    pub(crate) generation: String,
}
impl fmt::Debug for Pairing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pairing([redacted])")
    }
}
impl Pairing {
    pub fn parse(input: &str) -> Result<Self, Error> {
        if input.len() > 4096 {
            return Err(Error::InvalidPairing);
        }
        let url = url::Url::parse(input.trim()).map_err(|_| Error::InvalidPairing)?;
        if url.scheme() != "https"
            || url.host_str() != Some("companion.clip-studio.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.fragment().is_some()
            || !url.path().starts_with("/rc/")
            || url.path().len() <= 4
        {
            return Err(Error::InvalidPairing);
        }
        let mut parameters = url.query_pairs().filter(|(key, _)| key == "s");
        let (_, encoded) = parameters.next().ok_or(Error::InvalidPairing)?;
        if parameters.next().is_some() || encoded.is_empty() || encoded.len() % 2 != 0 {
            return Err(Error::InvalidPairing);
        }
        let bytes: Result<Vec<u8>, Error> = encoded
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .map(|(i, pair)| {
                let high = (pair[0] as char)
                    .to_digit(16)
                    .ok_or(Error::InvalidPairing)?;
                let low = (pair[1] as char)
                    .to_digit(16)
                    .ok_or(Error::InvalidPairing)?;
                Ok(((high * 16 + low) as u8) ^ QR_KEY[i % 7])
            })
            .collect();
        let decoded = String::from_utf8(bytes?).map_err(|_| Error::InvalidPairing)?;
        let parts: Vec<_> = decoded.split('\t').collect();
        if parts.len() != 4 || !printable(parts[2], 128) || !printable(parts[3], 64) {
            return Err(Error::InvalidPairing);
        }
        let port: u16 = parts[1].parse().map_err(|_| Error::InvalidPairing)?;
        if port == 0 || !parts[1].bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::InvalidPairing);
        }
        let mut endpoints = Vec::new();
        for address in parts[0].split(',') {
            let ip: IpAddr = address.parse().map_err(|_| Error::InvalidPairing)?;
            if ip.is_unspecified() || ip.is_multicast() {
                return Err(Error::InvalidPairing);
            }
            let endpoint = SocketAddr::new(ip, port);
            if !endpoints.contains(&endpoint) {
                endpoints.push(endpoint);
            }
            if endpoints.len() > 8 {
                return Err(Error::InvalidPairing);
            }
        }
        Ok(Self {
            endpoints,
            password: parts[2].to_owned(),
            generation: parts[3].to_owned(),
        })
    }
}
fn printable(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && value.bytes().all(|b| (32..=126).contains(&b))
}
pub(crate) fn auth_hex(value: &str) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(value.len() * 2);
    for (i, byte) in value.bytes().enumerate() {
        let encoded = byte ^ AUTH_KEY[i % 7];
        result.push(HEX[(encoded >> 4) as usize] as char);
        result.push(HEX[(encoded & 15) as usize] as char);
    }
    result
}
pub(crate) fn fresh_password() -> Result<String, Error> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(|_| Error::RandomUnavailable)?;
    Ok(bytes
        .iter()
        .map(|b| ALPHABET[(b & 63) as usize] as char)
        .collect())
}
