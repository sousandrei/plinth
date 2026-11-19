//! Framed exchanges over a mutually authenticated TCP connection.

use std::{
    fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    sync::Arc,
    time::Duration,
};

use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    server::WebPkiClientVerifier,
};

pub const TLS_NAME: &str = "plinth-peer.local";
const MAX_MESSAGE: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const ALPN: &[u8] = b"plinth-sync/1";

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Identity {
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
    trusted: CertificateDer<'static>,
}

impl Identity {
    pub fn load(dir: &Path) -> Result<Self> {
        Ok(Self {
            cert: fs::read(dir.join("peer.der"))?.into(),
            key: fs::read(dir.join("key.der"))?.into(),
            trusted: fs::read(dir.join("trusted-peer.der"))?.into(),
        })
    }

    fn roots(&self) -> Result<RootCertStore> {
        let mut roots = RootCertStore::empty();
        roots.add(self.trusted.clone())?;
        Ok(roots)
    }

    fn check_peer(&self, certs: Option<&[CertificateDer<'_>]>) -> Result<()> {
        if certs.and_then(|chain| chain.first()) != Some(&self.trusted) {
            return Err("peer certificate does not match the paired certificate".into());
        }
        Ok(())
    }
}

impl Identity {
    pub fn connect(&self, addr: SocketAddr) -> Result<impl Read + Write + use<>> {
        let mut config =
            ClientConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
                .with_safe_default_protocol_versions()?
                .with_root_certificates(self.roots()?)
                .with_client_auth_cert(vec![self.cert.clone()], self.key.clone_key().into())?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        let mut conn = ClientConnection::new(Arc::new(config), TLS_NAME.try_into()?)?;
        let mut socket = TcpStream::connect_timeout(&addr, TIMEOUT)?;
        timeouts(&socket)?;
        conn.complete_io(&mut socket)?;
        self.check_peer(conn.peer_certificates())?;
        check_protocol(conn.alpn_protocol())?;
        Ok(StreamOwned::new(conn, socket))
    }

    pub fn accept(&self, mut socket: TcpStream) -> Result<impl Read + Write + use<>> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::new(self.roots()?), provider.clone())
                .build()?;
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_client_cert_verifier(verifier)
            .with_single_cert(vec![self.cert.clone()], self.key.clone_key().into())?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        let mut conn = ServerConnection::new(Arc::new(config))?;
        timeouts(&socket)?;
        conn.complete_io(&mut socket)?;
        self.check_peer(conn.peer_certificates())?;
        check_protocol(conn.alpn_protocol())?;
        Ok(StreamOwned::new(conn, socket))
    }
}

fn check_protocol(protocol: Option<&[u8]>) -> Result<()> {
    if protocol != Some(ALPN) {
        return Err("unsupported sync protocol".into());
    }
    Ok(())
}

fn timeouts(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))
}

fn send(stream: &mut impl Write, message: Option<&[u8]>) -> io::Result<()> {
    let bytes = message.unwrap_or_default();
    if bytes.len() > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()
}

fn receive(stream: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut message = vec![0; length];
    stream.read_exact(&mut message)?;
    Ok(message)
}

/// Process each request before sending a reply. Empty frames end the exchange
/// once both peers have no further messages.
pub fn exchange(
    stream: &mut (impl Read + Write),
    space: &str,
    initiator: bool,
    mut step: impl FnMut(Option<&[u8]>) -> Result<Option<Vec<u8>>>,
) -> Result<()> {
    send(stream, Some(space.as_bytes()))?;
    if receive(stream)? != space.as_bytes() {
        return Err("paired peers selected different spaces".into());
    }
    if initiator {
        let mut outgoing = step(None)?;
        loop {
            send(stream, outgoing.as_deref())?;
            let incoming = receive(stream)?;
            if outgoing.is_none() && incoming.is_empty() {
                return Ok(());
            }
            outgoing = step((!incoming.is_empty()).then_some(incoming.as_slice()))?;
        }
    }
    loop {
        let incoming = receive(stream)?;
        let outgoing = step((!incoming.is_empty()).then_some(incoming.as_slice()))?;
        send(stream, outgoing.as_deref())?;
        if incoming.is_empty() && outgoing.is_none() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_keep_messages_and_end_markers_separate() {
        let mut stream = Vec::new();
        send(&mut stream, Some(b"edits")).unwrap();
        send(&mut stream, None).unwrap();
        let mut reader = stream.as_slice();
        assert_eq!(receive(&mut reader).unwrap(), b"edits");
        assert!(receive(&mut reader).unwrap().is_empty());
    }

    #[test]
    fn rejects_oversized_and_truncated_frames() {
        let oversized = ((MAX_MESSAGE + 1) as u32).to_be_bytes();
        assert_eq!(
            receive(&mut oversized.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let truncated = [0, 0, 0, 2, 1];
        assert_eq!(
            receive(&mut truncated.as_slice()).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}
