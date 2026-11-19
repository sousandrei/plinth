//! Two-process sync demo. Exchange the peers' public certificates in person
//! or over an authenticated channel before using `trust`.

use std::{
    env, fs,
    net::{SocketAddr, TcpListener},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use plinth_sync::transport::{self, Identity, Result};
use plinth_sync::{Store, discovery};
use rustls::{RootCertStore, pki_types::CertificateDer};

const SPACE: &str = "demo";

fn connect(dir: &Path, addr: SocketAddr) -> Result<()> {
    let mut stream = Identity::load(dir)?.connect(addr)?;
    transport::exchange_store(
        &mut Store::open(dir.join("sync.sqlite"))?,
        &mut stream,
        SPACE,
        true,
    )?;
    println!("SYNCED {addr}");
    Ok(())
}

fn listen(dir: &Path, addr: SocketAddr, name: Option<&str>) -> Result<()> {
    let identity = Identity::load(dir)?;
    let listener = TcpListener::bind(addr)?;
    let addr = listener.local_addr()?;
    let advertiser = name
        .map(|name| discovery::advertise(name, addr))
        .transpose()?;
    let result = (|| -> Result<()> {
        println!("READY {addr}");
        let (socket, remote) = listener.accept()?;
        let mut stream = identity.accept(socket)?;
        transport::exchange_store(
            &mut Store::open(dir.join("sync.sqlite"))?,
            &mut stream,
            SPACE,
            false,
        )?;
        println!("SYNCED {remote}");
        Ok(())
    })();
    if let Some(daemon) = advertiser {
        discovery::stop(&daemon);
    }
    result
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["init", dir] => Identity::init(Path::new(dir)),
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
            for addr in discovery::discover(name)? {
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
