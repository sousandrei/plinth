use std::{
    env,
    fs::OpenOptions,
    io::{self, Write},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use plinth_sync::{
    Store, discovery,
    transport::{self, Identity, Result},
};

fn serve(
    dir: &Path,
    space: &str,
    bind: SocketAddr,
    name: Option<&str>,
    certificates: &[PathBuf],
) -> Result<()> {
    if space.is_empty() {
        return Err("a shared space ID is required".into());
    }
    let identity = Identity::load_with_trusted(dir, certificates)?;
    let peer = identity.listener()?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("peer.lock"))?;
    lock.try_lock()?;
    let database = dir.join("sync.sqlite");
    let mut store = Store::open(&database)?;
    let listener = TcpListener::bind(bind)?;
    let address = listener.local_addr()?;
    let _advertisement = name
        .map(|name| discovery::advertise(name, address))
        .transpose()?;
    println!("READY {address}");
    io::stdout().flush()?;

    if let Err(error) = thread::Builder::new()
        .name("sync-checkpoint".into())
        .spawn(move || {
            loop {
                if let Err(error) =
                    Store::open(&database).and_then(|mut store| store.checkpoint_all())
                {
                    eprintln!("sync checkpoint: {error}");
                }
                thread::sleep(Duration::from_secs(24 * 60 * 60));
            }
        })
    {
        eprintln!("start sync checkpoint worker: {error}");
    }

    loop {
        let (socket, remote) = match listener.accept() {
            Ok(connection) => connection,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let exchange = peer.accept(socket).and_then(|mut stream| {
            transport::exchange_store(&mut store, &mut stream, space, false)
        });
        match exchange {
            Ok(()) => println!("SYNCED {remote}"),
            Err(error) => eprintln!("peer {remote}: {error}"),
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["init", dir] => {
            let dir = Path::new(dir);
            Identity::init(dir)?;
            println!("{}", dir.join("peer.der").display());
            Ok(())
        }
        ["serve", dir, space, bind, tail @ ..] => {
            let (name, certificates) = match tail {
                ["--mdns", name, certificates @ ..] => (Some(*name), certificates),
                _ => (None, tail),
            };
            let certificates = certificates.iter().map(PathBuf::from).collect::<Vec<_>>();
            serve(Path::new(dir), space, bind.parse()?, name, &certificates)
        }
        _ => Err("usage: plinth-sync-peer init DIR | serve DIR SPACE_ID IP:PORT [--mdns NAME] CERT [CERT ...]".into()),
    }
}
