//! Two-process sync demo. Exchange the peers' public certificates in person
//! or over an authenticated channel before using `trust`.

use std::{
    env, fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use plinth_sync::transport::{self, Identity, Result, TLS_NAME};
use plinth_sync::{Session, Store};
use rustls::{RootCertStore, pki_types::CertificateDer};

const SERVICE: &str = "_plinth-sync._tcp.local.";
const SPACE: &str = "demo";
const TIMEOUT: Duration = Duration::from_secs(5);

fn exchange(dir: &Path, stream: &mut (impl Read + Write), initiator: bool) -> Result<()> {
    let mut store = Store::open(dir.join("sync.sqlite"))?;
    let mut session = Session::new(SPACE);
    transport::exchange(stream, SPACE, initiator, |incoming| {
        if let Some(bytes) = incoming {
            store.receive(&mut session, bytes)?;
        }
        Ok(store.next_message(&mut session)?)
    })
}

fn connect(dir: &Path, addr: SocketAddr) -> Result<()> {
    let mut stream = Identity::load(dir)?.connect(addr)?;
    exchange(dir, &mut stream, true)?;
    println!("SYNCED {addr}");
    Ok(())
}

fn listen(dir: &Path, addr: SocketAddr, name: Option<&str>) -> Result<()> {
    let identity = Identity::load(dir)?;
    let listener = TcpListener::bind(addr)?;
    let addr = listener.local_addr()?;
    let advertiser = name.map(|name| advertise(name, addr)).transpose()?;
    let result = (|| -> Result<()> {
        println!("READY {addr}");
        let (socket, remote) = listener.accept()?;
        let mut stream = identity.accept(socket)?;
        exchange(dir, &mut stream, false)?;
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
