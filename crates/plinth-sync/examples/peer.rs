//! Two-process sync demo. Exchange the peers' public certificates in person
//! or over an authenticated channel before using `trust`.

use std::{
    env, fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use plinth_sync::{Session, Store};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    server::WebPkiClientVerifier,
};

const SERVICE: &str = "_plinth-sync._tcp.local.";
const TLS_NAME: &str = "plinth-peer.local";
const SPACE: &str = "demo";
const MAX_MESSAGE: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Identity {
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
    trusted: CertificateDer<'static>,
}

impl Identity {
    fn load(dir: &Path) -> Result<Self> {
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

fn exchange(store: &mut Store, stream: &mut (impl Read + Write), initiator: bool) -> Result<()> {
    let mut session = Session::new(SPACE);
    loop {
        // The listener replies only after committing any edits in the request.
        if initiator {
            let message = store.next_message(&mut session)?;
            send(stream, message.as_deref())?;
            let reply = receive(stream)?;
            if reply.is_empty() && message.is_none() {
                return Ok(());
            }
            if !reply.is_empty() {
                store.receive(&mut session, &reply)?;
            }
        } else {
            let request = receive(stream)?;
            if !request.is_empty() {
                store.receive(&mut session, &request)?;
            }
            let reply = store.next_message(&mut session)?;
            send(stream, reply.as_deref())?;
            if request.is_empty() && reply.is_none() {
                return Ok(());
            }
        }
    }
}

fn timeouts(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))
}

fn connect(dir: &Path, addr: SocketAddr) -> Result<()> {
    let identity = Identity::load(dir)?;
    let config = ClientConfig::builder()
        .with_root_certificates(identity.roots()?)
        .with_client_auth_cert(vec![identity.cert.clone()], identity.key.clone_key().into())?;
    let mut conn = ClientConnection::new(Arc::new(config), TLS_NAME.try_into()?)?;
    let mut socket = TcpStream::connect_timeout(&addr, TIMEOUT)?;
    timeouts(&socket)?;
    conn.complete_io(&mut socket)?;
    identity.check_peer(conn.peer_certificates())?;
    let mut stream = StreamOwned::new(conn, socket);
    exchange(
        &mut Store::open(dir.join("sync.sqlite"))?,
        &mut stream,
        true,
    )?;
    println!("SYNCED {addr}");
    Ok(())
}

fn listen(dir: &Path, addr: SocketAddr, name: Option<&str>) -> Result<()> {
    let identity = Identity::load(dir)?;
    let verifier = WebPkiClientVerifier::builder(Arc::new(identity.roots()?)).build()?;
    let config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![identity.cert.clone()], identity.key.clone_key().into())?;
    let listener = TcpListener::bind(addr)?;
    let addr = listener.local_addr()?;
    let advertiser = name.map(|name| advertise(name, addr)).transpose()?;
    let result = (|| -> Result<()> {
        println!("READY {addr}");
        let (mut socket, remote) = listener.accept()?;
        timeouts(&socket)?;
        let mut conn = ServerConnection::new(Arc::new(config))?;
        conn.complete_io(&mut socket)?;
        identity.check_peer(conn.peer_certificates())?;
        let mut stream = StreamOwned::new(conn, socket);
        exchange(
            &mut Store::open(dir.join("sync.sqlite"))?,
            &mut stream,
            false,
        )?;
        println!("SYNCED {remote}");
        Ok(())
    })();
    if let Some(daemon) = advertiser {
        stop_discovery(&daemon);
    }
    result
}

fn advertise(name: &str, addr: SocketAddr) -> Result<ServiceDaemon> {
    if name.is_empty()
        || name.len() > 48
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || !addr.is_ipv4()
        || addr.ip().is_loopback()
    {
        return Err("advertising requires an alphanumeric name and a LAN IPv4 bind".into());
    }

    let host = format!("plinth-{name}.local.");
    let info = if addr.ip().is_unspecified() {
        ServiceInfo::new(SERVICE, name, &host, "", addr.port(), None)?.enable_addr_auto()
    } else {
        ServiceInfo::new(SERVICE, name, &host, addr.ip(), addr.port(), None)?
    };
    let daemon = ServiceDaemon::new()?;
    daemon.register(info)?;
    Ok(daemon)
}

fn discover(name: &str) -> Result<Vec<SocketAddr>> {
    let fullname = format!("{name}.{SERVICE}");
    let daemon = ServiceDaemon::new()?;
    let result = (|| -> Result<_> {
        let events = daemon.browse(SERVICE)?;
        let deadline = Instant::now() + TIMEOUT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let Ok(event) = events.recv_timeout(remaining) else {
                break;
            };
            if let ServiceEvent::ServiceResolved(info) = event
                && info.get_fullname().eq_ignore_ascii_case(&fullname)
            {
                let addrs: Vec<_> = info
                    .get_addresses_v4()
                    .into_iter()
                    .map(|ip| SocketAddr::new(ip.into(), info.get_port()))
                    .collect();
                if !addrs.is_empty() {
                    return Ok(addrs);
                }
            }
        }
        Err(format!("no mDNS service named {name} found").into())
    })();
    stop_discovery(&daemon);
    result
}

fn stop_discovery(daemon: &ServiceDaemon) {
    if let Ok(status) = daemon.shutdown() {
        let _ = status.recv_timeout(TIMEOUT);
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["init", dir] => {
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            builder.create(dir)?;
            let dir = Path::new(dir);
            let identity = rcgen::generate_simple_self_signed(vec![TLS_NAME.into()])?;
            fs::write(dir.join("peer.der"), identity.cert.der())?;
            fs::write(dir.join("key.der"), identity.signing_key.serialize_der())?;
            Ok(())
        }
        ["trust", dir, cert] => {
            let cert = fs::read(cert)?;
            RootCertStore::empty().add(CertificateDer::from(cert.as_slice()))?;
            fs::write(Path::new(dir).join("trusted-peer.der"), cert)?;
            Ok(())
        }
        ["edit", dir, record, field, value] => {
            let mut store = Store::open(Path::new(dir).join("sync.sqlite"))?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis().try_into()?;
            store.patch(SPACE, record, now, &[(field.to_string(), (*value).into())].into())?;
            Ok(())
        }
        ["show", dir] => {
            let store = Store::open(Path::new(dir).join("sync.sqlite"))?;
            println!("{}", serde_json::to_string_pretty(&store.snapshot(SPACE)?)?);
            Ok(())
        }
        ["listen", dir, addr] => listen(Path::new(dir), addr.parse()?, None),
        ["listen", dir, addr, name] => listen(Path::new(dir), addr.parse()?, Some(name)),
        ["connect", dir, addr] => connect(Path::new(dir), addr.parse()?),
        ["discover", dir, name] => {
            for addr in discover(name)? {
                println!("DISCOVERED {name} {addr}");
                match connect(Path::new(dir), addr) {
                    Ok(()) => return Ok(()),
                    Err(error) => eprintln!("{addr}: {error}"),
                }
            }
            Err("no discovered address connected successfully".into())
        }
        _ => Err("usage: peer init DIR | trust DIR CERT | edit DIR RECORD FIELD VALUE | show DIR | listen DIR IP:PORT [NAME] | connect DIR IP:PORT | discover DIR NAME".into()),
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
