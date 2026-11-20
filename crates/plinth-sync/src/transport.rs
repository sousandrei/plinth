//! Framed exchanges over a mutually authenticated TCP connection.

use std::{
    fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    server::WebPkiClientVerifier,
};

use crate::{Session, Store};

pub const TLS_NAME: &str = "plinth-peer.local";
const MAX_MESSAGE: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const ALPN: &[u8] = b"plinth-sync/1";
const PERMISSION_ALPN: &[u8] = b"plinth-permissions/1";
const ROSTER_ALPN: &[u8] = b"plinth-roster/1";
const ENROLLMENT_ALPN: &[u8] = b"plinth-enrollment/3";
pub const MAX_ENROLLMENT_REQUEST: usize = 8 * 1024;
pub const MAX_SPACE_SNAPSHOT: usize = 128 * 1024 * 1024;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Identity {
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
    trusted: Vec<CertificateDer<'static>>,
}

pub struct PeerListener<'a> {
    identity: &'a Identity,
    config: Arc<ServerConfig>,
    protocols: &'static [PeerProtocol],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerProtocol {
    Data,
    Permissions,
    Roster,
}

impl PeerProtocol {
    fn alpn(self) -> &'static [u8] {
        match self {
            Self::Data => ALPN,
            Self::Permissions => PERMISSION_ALPN,
            Self::Roster => ROSTER_ALPN,
        }
    }
}

impl Identity {
    pub fn init(dir: &Path) -> Result<()> {
        let directory = fs::DirBuilder::new();
        #[cfg(unix)]
        let directory = {
            use std::os::unix::fs::DirBuilderExt;
            let mut directory = directory;
            directory.mode(0o700);
            directory
        };
        let identity = rcgen::generate_simple_self_signed(vec![TLS_NAME.into()])?;
        directory.create(dir)?;
        fs::write(dir.join("key.der"), identity.signing_key.serialize_der())?;
        fs::write(dir.join("peer.der"), identity.cert.der())?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        Self::load_with_trusted(dir, &[dir.join("trusted-peer.der")])
    }

    pub fn load_with_trusted(dir: &Path, certificates: &[PathBuf]) -> Result<Self> {
        if certificates.is_empty() {
            return Err("at least one paired certificate is required".into());
        }
        let trusted = certificates
            .iter()
            .map(|path| fs::read(path).map(CertificateDer::from))
            .collect::<io::Result<_>>()?;
        Self::with_certificates(dir, trusted)
    }

    pub fn with_certificates(dir: &Path, trusted: Vec<CertificateDer<'static>>) -> Result<Self> {
        Ok(Self {
            cert: fs::read(dir.join("peer.der"))?.into(),
            key: fs::read(dir.join("key.der"))?.into(),
            trusted,
        })
    }

    pub fn certificate(&self) -> &[u8] {
        self.cert.as_ref()
    }

    pub fn accept_enrollment(
        &self,
        mut socket: TcpStream,
    ) -> Result<(impl Read + Write + use<>, Vec<u8>, [u8; 32])> {
        timeouts(&socket)?;
        write_frame(&mut socket, self.certificate(), MAX_ENROLLMENT_REQUEST)?;
        let certificate = read_frame(&mut socket, MAX_ENROLLMENT_REQUEST)?;
        let cert = CertificateDer::from(certificate.as_slice());
        if cert == self.cert {
            return Err("cannot pair a device with itself".into());
        }
        let mut roots = RootCertStore::empty();
        roots.add(cert.clone())?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
                .build()?;
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_client_cert_verifier(verifier)
            .with_single_cert(vec![self.cert.clone()], self.key.clone_key().into())?;
        config.alpn_protocols = vec![ENROLLMENT_ALPN.to_vec()];
        let mut conn = ServerConnection::new(Arc::new(config))?;
        conn.complete_io(&mut socket)?;
        if conn.peer_certificates().and_then(|certs| certs.first()) != Some(&cert) {
            return Err("pairing device presented a different certificate".into());
        }
        check_protocol(conn.alpn_protocol(), ENROLLMENT_ALPN)?;
        let binding = conn.export_keying_material([0; 32], b"plinth pairing confirmation", None)?;
        enrollment_timeouts(&socket)?;
        Ok((StreamOwned::new(conn, socket), certificate, binding))
    }

    // TLS proves that each device holds its offered key. The confirmation code
    // binds the user's approval to that specific connection.
    pub fn connect_enrollment(
        &self,
        addr: SocketAddr,
    ) -> Result<(impl Read + Write + use<>, Vec<u8>, [u8; 32])> {
        let mut socket = TcpStream::connect_timeout(&addr, TIMEOUT)?;
        timeouts(&socket)?;
        let certificate = read_frame(&mut socket, MAX_ENROLLMENT_REQUEST)?;
        let cert = CertificateDer::from(certificate.as_slice());
        if cert == self.cert {
            return Err("cannot pair a device with itself".into());
        }
        let mut roots = RootCertStore::empty();
        roots.add(cert.clone())?;
        write_frame(&mut socket, self.certificate(), MAX_ENROLLMENT_REQUEST)?;
        let mut config =
            ClientConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
                .with_safe_default_protocol_versions()?
                .with_root_certificates(roots)
                .with_client_auth_cert(vec![self.cert.clone()], self.key.clone_key().into())?;
        config.alpn_protocols = vec![ENROLLMENT_ALPN.to_vec()];
        let mut conn = ClientConnection::new(Arc::new(config), TLS_NAME.try_into()?)?;
        conn.complete_io(&mut socket)?;
        if conn.peer_certificates().and_then(|certs| certs.first()) != Some(&cert) {
            return Err("pairing host presented a different certificate".into());
        }
        check_protocol(conn.alpn_protocol(), ENROLLMENT_ALPN)?;
        let binding = conn.export_keying_material([0; 32], b"plinth pairing confirmation", None)?;
        enrollment_timeouts(&socket)?;
        Ok((StreamOwned::new(conn, socket), certificate, binding))
    }

    fn roots(&self) -> Result<RootCertStore> {
        let mut roots = RootCertStore::empty();
        for certificate in &self.trusted {
            roots.add(certificate.clone())?;
        }
        Ok(roots)
    }

    fn check_peer(&self, certs: Option<&[CertificateDer<'_>]>) -> Result<()> {
        if !certs
            .and_then(|chain| chain.first())
            .is_some_and(|cert| self.trusted.contains(cert))
        {
            return Err("peer certificate does not match the paired certificate".into());
        }
        Ok(())
    }
}

impl Identity {
    pub fn connect_any(&self, addresses: Vec<SocketAddr>) -> Result<impl Read + Write + use<>> {
        let mut last_error = "no peer address was resolved".into();
        for address in addresses {
            match self.connect(address) {
                Ok(stream) => return Ok(stream),
                Err(error) => last_error = format!("{address}: {error}").into(),
            }
        }
        Err(last_error)
    }

    pub fn connect(&self, addr: SocketAddr) -> Result<impl Read + Write + use<>> {
        self.connect_protocol(addr, PeerProtocol::Data, TIMEOUT)
    }

    pub(crate) fn connect_protocol(
        &self,
        addr: SocketAddr,
        protocol: PeerProtocol,
        timeout: Duration,
    ) -> Result<impl Read + Write + use<>> {
        let mut config =
            ClientConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
                .with_safe_default_protocol_versions()?
                .with_root_certificates(self.roots()?)
                .with_client_auth_cert(vec![self.cert.clone()], self.key.clone_key().into())?;
        config.alpn_protocols = vec![protocol.alpn().to_vec()];
        let mut conn = ClientConnection::new(Arc::new(config), TLS_NAME.try_into()?)?;
        let mut socket = TcpStream::connect_timeout(&addr, timeout)?;
        socket.set_read_timeout(Some(timeout))?;
        socket.set_write_timeout(Some(timeout))?;
        conn.complete_io(&mut socket)?;
        self.check_peer(conn.peer_certificates())?;
        check_protocol(conn.alpn_protocol(), protocol.alpn())?;
        Ok(StreamOwned::new(conn, socket))
    }

    pub fn listener(&self) -> Result<PeerListener<'_>> {
        self.listen_for(&[PeerProtocol::Data])
    }

    pub fn listener_with_permissions(&self) -> Result<PeerListener<'_>> {
        self.listen_for(&[PeerProtocol::Data, PeerProtocol::Permissions])
    }

    pub fn listener_with_roster(&self) -> Result<PeerListener<'_>> {
        self.listen_for(&[PeerProtocol::Data, PeerProtocol::Roster])
    }

    pub fn connect_roster(&self, addr: SocketAddr) -> Result<impl Read + Write + use<>> {
        self.connect_protocol(addr, PeerProtocol::Roster, TIMEOUT)
    }

    fn listen_for(&self, protocols: &'static [PeerProtocol]) -> Result<PeerListener<'_>> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::new(self.roots()?), provider.clone())
                .build()?;
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_client_cert_verifier(verifier)
            .with_single_cert(vec![self.cert.clone()], self.key.clone_key().into())?;
        config.alpn_protocols = protocols
            .iter()
            .map(|protocol| protocol.alpn().to_vec())
            .collect();
        Ok(PeerListener {
            identity: self,
            config: Arc::new(config),
            protocols,
        })
    }

    pub fn accept(&self, socket: TcpStream) -> Result<impl Read + Write + use<>> {
        self.listener()?.accept(socket)
    }
}

impl PeerListener<'_> {
    pub fn accept(&self, socket: TcpStream) -> Result<impl Read + Write + use<>> {
        let (stream, _) = self.accept_identified(socket)?;
        Ok(stream)
    }

    pub fn accept_identified(
        &self,
        socket: TcpStream,
    ) -> Result<(impl Read + Write + use<>, Vec<u8>)> {
        let (stream, certificate, protocol) = self.accept_protocol(socket)?;
        if protocol != PeerProtocol::Data {
            return Err("unexpected peer protocol".into());
        }
        Ok((stream, certificate))
    }

    pub fn accept_protocol(
        &self,
        mut socket: TcpStream,
    ) -> Result<(impl Read + Write + use<>, Vec<u8>, PeerProtocol)> {
        let mut conn = ServerConnection::new(self.config.clone())?;
        timeouts(&socket)?;
        conn.complete_io(&mut socket)?;
        self.identity.check_peer(conn.peer_certificates())?;
        let protocol = self
            .protocols
            .iter()
            .copied()
            .find(|protocol| Some(protocol.alpn()) == conn.alpn_protocol())
            .ok_or("unsupported sync protocol")?;
        let certificate = conn
            .peer_certificates()
            .and_then(|certs| certs.first())
            .ok_or("missing peer certificate")?
            .to_vec();
        Ok((StreamOwned::new(conn, socket), certificate, protocol))
    }
}

pub fn exchange_store(
    store: &mut Store,
    stream: &mut (impl Read + Write),
    space: &str,
    initiator: bool,
) -> Result<()> {
    let mut session = Session::new(space);
    exchange(stream, space, initiator, |incoming| {
        if let Some(bytes) = incoming {
            store.receive(&mut session, bytes)?;
        }
        Ok(store.next_message(&mut session)?)
    })
}

fn check_protocol(protocol: Option<&[u8]>, expected: &[u8]) -> Result<()> {
    if protocol != Some(expected) {
        return Err("unsupported sync protocol".into());
    }
    Ok(())
}

fn timeouts(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))
}

fn enrollment_timeouts(socket: &TcpStream) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(120)))?;
    socket.set_write_timeout(Some(TIMEOUT))
}

fn send(stream: &mut impl Write, message: Option<&[u8]>) -> io::Result<()> {
    write_frame(stream, message.unwrap_or_default(), MAX_MESSAGE)
}

pub fn write_frame(stream: &mut impl Write, bytes: &[u8], limit: usize) -> io::Result<()> {
    if bytes.len() > limit || bytes.len() > u32::MAX as usize {
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
    read_frame(stream, MAX_MESSAGE)
}

pub fn read_frame(stream: &mut impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > limit {
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
    step: impl FnMut(Option<&[u8]>) -> Result<Option<Vec<u8>>>,
) -> Result<()> {
    send(stream, Some(space.as_bytes()))?;
    if receive(stream)? != space.as_bytes() {
        return Err("paired peers selected different spaces".into());
    }
    exchange_messages(stream, initiator, step)
}

/// Read the initiator's space before releasing any data to the authenticated peer.
pub fn read_space(stream: &mut impl Read) -> Result<String> {
    let space = String::from_utf8(read_frame(stream, 256)?)?;
    if space.is_empty() {
        return Err("missing sync space".into());
    }
    Ok(space)
}

pub fn respond(
    stream: &mut (impl Read + Write),
    space: &str,
    step: impl FnMut(Option<&[u8]>) -> Result<Option<Vec<u8>>>,
) -> Result<()> {
    send(stream, Some(space.as_bytes()))?;
    exchange_messages(stream, false, step)
}

fn exchange_messages(
    stream: &mut (impl Read + Write),
    initiator: bool,
    mut step: impl FnMut(Option<&[u8]>) -> Result<Option<Vec<u8>>>,
) -> Result<()> {
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
    use std::{net::TcpListener, thread};

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

    #[test]
    #[ignore = "requires localhost TCP"]
    fn enrollment_derives_the_same_binding_on_both_peers() {
        let temp = tempfile::tempdir().unwrap();
        let host_dir = temp.path().join("host");
        let client_dir = temp.path().join("client");
        Identity::init(&host_dir).unwrap();
        Identity::init(&client_dir).unwrap();
        let host = Identity::with_certificates(&host_dir, vec![]).unwrap();
        let client = Identity::with_certificates(&client_dir, vec![]).unwrap();
        let certificate = host.certificate().to_vec();
        let client_certificate = client.certificate().to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let (mut stream, certificate, binding) = host.accept_enrollment(socket).unwrap();
            assert_eq!(certificate, client_certificate);
            assert_eq!(
                read_frame(&mut stream, MAX_ENROLLMENT_REQUEST).unwrap(),
                b"request"
            );
            write_frame(&mut stream, b"seed", MAX_SPACE_SNAPSHOT).unwrap();
            binding
        });
        let (mut client, received_cert, binding) = client.connect_enrollment(address).unwrap();
        assert_eq!(received_cert, certificate);
        write_frame(&mut client, b"request", MAX_ENROLLMENT_REQUEST).unwrap();
        assert_eq!(
            read_frame(&mut client, MAX_SPACE_SNAPSHOT).unwrap(),
            b"seed"
        );
        assert_eq!(server.join().unwrap(), binding);
    }

    #[test]
    #[ignore = "requires localhost TCP"]
    fn enrollment_rejects_an_offered_certificate_without_its_key() {
        let temp = tempfile::tempdir().unwrap();
        let host_dir = temp.path().join("host");
        let client_dir = temp.path().join("client");
        Identity::init(&host_dir).unwrap();
        Identity::init(&client_dir).unwrap();
        let host = Identity::with_certificates(&host_dir, vec![]).unwrap();
        let client = Identity::with_certificates(&client_dir, vec![]).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            assert!(host.accept_enrollment(socket).is_err());
        });

        let mut socket = TcpStream::connect(address).unwrap();
        timeouts(&socket).unwrap();
        let cert = read_frame(&mut socket, MAX_ENROLLMENT_REQUEST).unwrap();
        write_frame(&mut socket, client.certificate(), MAX_ENROLLMENT_REQUEST).unwrap();
        let mut roots = RootCertStore::empty();
        roots.add(cert.into()).unwrap();
        let mut config =
            ClientConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();
        config.alpn_protocols = vec![ENROLLMENT_ALPN.to_vec()];
        let mut conn =
            ClientConnection::new(Arc::new(config), TLS_NAME.try_into().unwrap()).unwrap();
        let _ = conn.complete_io(&mut socket);
        server.join().unwrap();
    }
}
