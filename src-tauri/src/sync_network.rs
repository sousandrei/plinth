use std::{
    net::{IpAddr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use plinth_sync::{
    PairedDevice, Session as PeerSession, discovery,
    transport::{self, Identity},
};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tauri::{AppHandle, Emitter, State};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinHandle};

use crate::{
    Session,
    commands::spaces::{self, SyncSeed},
    db::DbPool,
    error::AppError,
    sync_preview::{PeerConnection, SyncPreview, resolve_peer},
};

const INVITATION_TTL: Duration = Duration::from_secs(300);
const SYNC_INTERVAL: Duration = Duration::from_secs(5);
const SYNC_PORT: u16 = 37419;
const INVITE_PORT: u16 = 37420;
const INVITATION_PREFIX: &str = "plinth1.";

#[derive(Serialize, Deserialize)]
struct Invitation {
    name: String,
    port: u16,
    certificate: Vec<u8>,
    token: [u8; 24],
}

#[derive(Serialize, Deserialize)]
struct Enrollment {
    token: [u8; 24],
    certificate: Vec<u8>,
    name: String,
}

#[derive(Serialize, Deserialize)]
struct Bootstrap {
    seed: SyncSeed,
    port: u16,
    name: String,
}

struct Offer {
    space_id: String,
    user_id: String,
    token: [u8; 24],
    deadline: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInvitation {
    pub code: String,
    pub name: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDevice {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub last_success: Option<i64>,
    pub error: Option<String>,
}

#[derive(Default)]
struct DeviceStatus {
    last_success: Option<i64>,
    error: Option<String>,
}

pub struct SyncNetwork {
    inner: Arc<Network>,
    workers: [JoinHandle<()>; 2],
}

struct Network {
    directory: PathBuf,
    name: String,
    port: u16,
    invite: Mutex<Option<JoinHandle<()>>>,
    status: Mutex<std::collections::HashMap<(String, String), DeviceStatus>>,
    app: Option<AppHandle>,
}

impl SyncNetwork {
    pub(crate) async fn start(
        directory: &Path,
        db: DbPool,
        store: SyncPreview,
        app: AppHandle,
    ) -> Result<Self, AppError> {
        let (inner, listener) = Network::open(directory, SYNC_PORT, Some(app)).await?;
        let workers = [
            tokio::spawn(inner.clone().listen(listener, db.clone(), store.clone())),
            tokio::spawn(inner.clone().sync_loop(db, store)),
        ];
        Ok(Self { inner, workers })
    }

    pub(crate) fn stop(&self) {
        self.workers.iter().for_each(JoinHandle::abort);
        if let Ok(mut invitation) = self.inner.invite.lock()
            && let Some(task) = invitation.take()
        {
            task.abort();
        }
    }
}

impl Network {
    async fn open(
        directory: &Path,
        port: u16,
        app: Option<AppHandle>,
    ) -> Result<(Arc<Self>, TcpListener), AppError> {
        let directory = directory.join("sync-device");
        let identity_dir = directory.clone();
        let identity = tokio::task::spawn_blocking(move || {
            if !identity_dir.exists() {
                Identity::init(&identity_dir).map_err(network_error)?;
            }
            Identity::with_certificates(&identity_dir, vec![]).map_err(network_error)
        })
        .await
        .map_err(join_error)??;
        let name = format!("plinth-{}", &certificate_id(identity.certificate())[..12]);
        let listener = bind(port).await?;
        let port = listener.local_addr().map_err(io_error)?.port();
        let inner = Arc::new(Self {
            directory,
            name,
            port,
            invite: Mutex::new(None),
            status: Mutex::default(),
            app,
        });
        Ok((inner, listener))
    }
}

#[cfg(test)]
#[path = "sync_network_tests.rs"]
mod tests;

#[tauri::command]
pub async fn create_sync_invitation(
    space_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    store: State<'_, SyncPreview>,
    network: State<'_, SyncNetwork>,
) -> Result<SyncInvitation, AppError> {
    let user = session.require_user()?;
    spaces::require_owner(&space_id, &user.user_id, db.inner()).await?;
    network
        .inner
        .clone()
        .invite(
            db.inner().clone(),
            store.inner().clone(),
            space_id,
            user.user_id,
        )
        .await
}

#[tauri::command]
pub async fn join_shared_space(
    code: String,
    address: Option<String>,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    store: State<'_, SyncPreview>,
    network: State<'_, SyncNetwork>,
) -> Result<String, AppError> {
    let user = session.require_user()?;
    network
        .inner
        .join(
            db.inner(),
            store.inner(),
            &user.user_id,
            code.trim(),
            address,
        )
        .await
}

#[tauri::command]
pub async fn list_sync_devices(
    space_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    store: State<'_, SyncPreview>,
    network: State<'_, SyncNetwork>,
) -> Result<Vec<SyncDevice>, AppError> {
    let user = session.require_user()?;
    spaces::require_member(&space_id, &user.user_id, db.inner()).await?;
    let peers = store.with_store(|store| store.paired_devices()).await?;
    let status = network.inner.status.lock().map_err(|_| state_error())?;
    Ok(peers
        .into_iter()
        .filter(|peer| peer.space_id == space_id)
        .map(|peer| {
            let id = certificate_id(&peer.certificate);
            let state = status.get(&(space_id.clone(), id.clone()));
            SyncDevice {
                id,
                name: peer.name,
                address: peer.address,
                last_success: state.and_then(|state| state.last_success),
                error: state.and_then(|state| state.error.clone()),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn remove_sync_device(
    space_id: String,
    device_id: String,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    store: State<'_, SyncPreview>,
) -> Result<(), AppError> {
    let user = session.require_user()?;
    spaces::require_owner(&space_id, &user.user_id, db.inner()).await?;
    store
        .with_store(move |store| {
            if let Some(peer) = store.paired_devices()?.into_iter().find(|peer| {
                peer.space_id == space_id && certificate_id(&peer.certificate) == device_id
            }) {
                store.unpair_device(&space_id, &peer.certificate)?;
            }
            Ok(())
        })
        .await
}

impl Network {
    async fn invite(
        self: Arc<Self>,
        db: DbPool,
        store: SyncPreview,
        space_id: String,
        user_id: String,
    ) -> Result<SyncInvitation, AppError> {
        let identity = open_identity(self.directory.clone()).await?;
        let mut token = [0; 24];
        getrandom::fill(&mut token).map_err(|error| AppError::Internal(error.to_string()))?;
        // Replace the previous invitation so this space is the only one offered.
        if let Some(previous) = self.invite.lock().map_err(|_| state_error())?.take() {
            previous.abort();
        }
        let listener = bind(INVITE_PORT).await?;
        let address = listener.local_addr().map_err(io_error)?;
        let name = format!("join-{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
        let advertisement = discovery::advertise_managed(&name, address).ok();
        let invitation = Invitation {
            name: name.clone(),
            port: address.port(),
            certificate: identity.certificate().to_vec(),
            token,
        };
        let code = format!(
            "{INVITATION_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(encode(&invitation)?)
        );
        let network = self.clone();
        let task = tokio::spawn(async move {
            let _advertisement = advertisement;
            let offer = Offer {
                space_id,
                user_id,
                token: invitation.token,
                deadline: Instant::now() + INVITATION_TTL,
            };
            while let Some(remaining) = offer.deadline.checked_duration_since(Instant::now()) {
                let Ok(Ok((socket, _))) = tokio::time::timeout(remaining, listener.accept()).await
                else {
                    break;
                };
                if let Ok(socket) = blocking_socket(socket) {
                    let operation = network.clone().enroll(&db, &store, &offer, socket);
                    if let Ok(true) = operation.await {
                        break;
                    }
                }
            }
        });
        let mut guard = self.invite.lock().map_err(|_| state_error())?;
        if let Some(previous) = guard.replace(task) {
            previous.abort();
        }
        Ok(SyncInvitation {
            code,
            name,
            port: address.port(),
        })
    }

    async fn enroll(
        self: Arc<Self>,
        db: &DbPool,
        store: &SyncPreview,
        offer: &Offer,
        socket: TcpStream,
    ) -> Result<bool, AppError> {
        let identity_dir = self.directory.clone();
        let Offer {
            token, deadline, ..
        } = *offer;
        let (mut stream, request) = tokio::task::spawn_blocking(move || {
            let identity =
                Identity::with_certificates(&identity_dir, vec![]).map_err(network_error)?;
            let mut stream = identity.accept_enrollment(socket).map_err(network_error)?;
            let request: Enrollment = decode(
                &transport::read_frame(&mut stream, transport::MAX_ENROLLMENT_REQUEST)
                    .map_err(io_error)?,
            )?;
            if Instant::now() >= deadline
                || !bool::from(request.token.ct_eq(&token))
                || request.certificate == identity.certificate()
            {
                return Err(AppError::Forbidden);
            }
            Identity::validate_certificate(&request.certificate).map_err(network_error)?;
            Ok((stream, request))
        })
        .await
        .map_err(join_error)??;

        let (snapshot, seed) =
            spaces::prepare_sync_seed_export(db, &offer.space_id, &offer.user_id).await?;
        let data = encode(&Bootstrap {
            seed,
            port: self.port,
            name: self.name.clone(),
        })?;
        if data.len() > transport::MAX_SPACE_SNAPSHOT {
            return Err(AppError::InvalidInput(
                "space snapshot exceeds the transfer limit".into(),
            ));
        }
        let peer = PairedDevice {
            space_id: offer.space_id.clone(),
            certificate: request.certificate,
            name: device_name(&request.name)?,
            address: None,
        };
        store
            .with_store(move |store| store.pair_device(&peer))
            .await?;
        snapshot.commit().await?;
        tokio::task::spawn_blocking(move || {
            transport::write_frame(&mut stream, &data, transport::MAX_SPACE_SNAPSHOT)
                .map_err(io_error)
        })
        .await
        .map_err(join_error)??;
        self.changed(&offer.space_id);
        Ok(true)
    }

    async fn join(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        user_id: &str,
        code: &str,
        address: Option<String>,
    ) -> Result<String, AppError> {
        let invitation = parse_invitation(code)?;
        let address = address
            .filter(|value| !value.trim().is_empty())
            .map(|value| address_with_port(value.trim(), invitation.port))
            .unwrap_or_else(|| format!("mdns:{}", invitation.name));
        let candidates = resolve_peer(address).await?;
        let identity = open_identity(self.directory.clone()).await?;
        let request = encode(&Enrollment {
            token: invitation.token,
            certificate: identity.certificate().to_vec(),
            name: self.name.clone(),
        })?;
        let certificate = invitation.certificate;
        let (ip, bootstrap, certificate) = tokio::task::spawn_blocking(move || {
            let mut error = AppError::Io("no enrollment address available".into());
            for address in candidates {
                match Identity::connect_enrollment(address, &certificate) {
                    Ok(mut stream) => {
                        transport::write_frame(
                            &mut stream,
                            &request,
                            transport::MAX_ENROLLMENT_REQUEST,
                        )
                        .map_err(io_error)?;
                        let data =
                            transport::read_frame(&mut stream, transport::MAX_SPACE_SNAPSHOT)
                                .map_err(io_error)?;
                        return Ok((address.ip(), decode::<Bootstrap>(&data)?, certificate));
                    }
                    Err(failure) => error = network_error(failure),
                }
            }
            Err(error)
        })
        .await
        .map_err(join_error)??;
        if bootstrap.port == 0 {
            return Err(AppError::InvalidInput("missing peer sync port".into()));
        }
        let space_id = bootstrap.seed.space_id.clone();
        let peer = PairedDevice {
            space_id: space_id.clone(),
            certificate,
            name: device_name(&bootstrap.name)?,
            address: Some(SocketAddr::new(ip, bootstrap.port).to_string()),
        };
        let install = spaces::prepare_sync_seed(db, user_id, bootstrap.seed).await?;
        store
            .with_store(move |store| store.pair_device(&peer))
            .await?;
        install.commit().await?;
        self.changed(&space_id);
        Ok(space_id)
    }

    async fn listen(self: Arc<Self>, listener: TcpListener, db: DbPool, store: SyncPreview) {
        let _advertisement = listener
            .local_addr()
            .ok()
            .and_then(|address| discovery::advertise_managed(&self.name, address).ok());
        let slots = Arc::new(Semaphore::new(8));
        while let Ok((socket, _)) = listener.accept().await {
            let Ok(permit) = slots.clone().try_acquire_owned() else {
                continue;
            };
            let Ok(socket) = blocking_socket(socket) else {
                continue;
            };
            let network = self.clone();
            let db = db.clone();
            let store = store.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let _ = network.accept(&db, &store, socket).await;
            });
        }
    }

    async fn accept(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        socket: TcpStream,
    ) -> Result<(), AppError> {
        let peers = store.with_store(|store| store.paired_devices()).await?;
        let mut certificates: Vec<_> = peers.iter().map(|peer| peer.certificate.clone()).collect();
        certificates.sort_unstable();
        certificates.dedup();
        if certificates.is_empty() {
            return Err(AppError::Forbidden);
        }
        let directory = self.directory.clone();
        let (mut stream, certificate, space_id) = tokio::task::spawn_blocking(move || {
            let identity = Identity::with_certificates(
                &directory,
                certificates.into_iter().map(Into::into).collect(),
            )
            .map_err(network_error)?;
            let (mut stream, certificate) = identity
                .listener()
                .map_err(network_error)?
                .accept_identified(socket)
                .map_err(network_error)?;
            let space = transport::read_space(&mut stream).map_err(network_error)?;
            Ok::<_, AppError>((stream, certificate, space))
        })
        .await
        .map_err(join_error)??;
        let peer = self
            .current_peer(db, store, &space_id, &certificate)
            .await?;
        let db = db.clone();
        let store = store.clone();
        let runtime = tokio::runtime::Handle::current();
        let space = space_id.clone();
        let revision = store.revision();
        let store_for_exchange = store.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut session = PeerSession::new(&space);
            transport::respond(&mut stream, &space, |incoming| {
                let (next, outgoing) = runtime.block_on(store_for_exchange.exchange_step(
                    &db,
                    std::mem::replace(&mut session, PeerSession::new(&space)),
                    incoming.map(<[u8]>::to_vec),
                ))?;
                session = next;
                Ok(outgoing)
            })
            .map_err(network_error)
        })
        .await
        .map_err(join_error)?;
        self.record_status(&peer, &result);
        if store.revision() != revision {
            self.changed(&space_id);
        }
        result
    }

    async fn current_peer(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        space_id: &str,
        certificate: &[u8],
    ) -> Result<PairedDevice, AppError> {
        let exists = sqlx::query_file!("queries/spaces/get_space_name.sql", space_id)
            .fetch_optional(db)
            .await?
            .is_some();
        if !exists {
            return Err(AppError::Forbidden);
        }
        store
            .with_store(|store| store.paired_devices())
            .await?
            .into_iter()
            .find(|peer| peer.space_id == space_id && peer.certificate == certificate)
            .ok_or(AppError::Forbidden)
    }

    async fn sync_loop(self: Arc<Self>, db: DbPool, store: SyncPreview) {
        loop {
            if let Ok(peers) = store.with_store(|store| store.paired_devices()).await {
                let mut tasks = tokio::task::JoinSet::new();
                for peer in peers.into_iter().filter(|peer| peer.address.is_some()) {
                    if tasks.len() >= 8 {
                        let _ = tasks.join_next().await;
                    }
                    let db = db.clone();
                    let store = store.clone();
                    let network = self.clone();
                    tasks.spawn(async move { network.sync_peer(&db, &store, peer).await });
                }
                while tasks.join_next().await.is_some() {}
            }
            tokio::time::sleep(SYNC_INTERVAL).await;
        }
    }

    async fn sync_peer(&self, db: &DbPool, store: &SyncPreview, peer: PairedDevice) {
        let Ok(peer) = self
            .current_peer(db, store, &peer.space_id, &peer.certificate)
            .await
        else {
            return;
        };
        let Some(address) = &peer.address else { return };
        let revision = store.revision();
        let mut result = self.dial(db, store, &peer, address.clone()).await;
        if result.is_err() {
            result = self
                .dial(db, store, &peer, format!("mdns:{}", peer.name))
                .await;
        }
        self.record_status(&peer, &result);
        if store.revision() != revision {
            self.changed(&peer.space_id);
        }
    }

    async fn dial(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        peer: &PairedDevice,
        address: String,
    ) -> Result<(), AppError> {
        let addresses = resolve_peer(address).await?;
        let directory = self.directory.clone();
        let certificate = peer.certificate.clone();
        let identity = tokio::task::spawn_blocking(move || {
            Identity::with_certificates(&directory, vec![certificate.into()]).map_err(network_error)
        })
        .await
        .map_err(join_error)??;
        store
            .exchange_peer(
                db,
                peer.space_id.clone(),
                identity,
                PeerConnection::Connect(addresses),
            )
            .await
    }

    fn record_status(&self, peer: &PairedDevice, result: &Result<(), AppError>) {
        if let Ok(mut states) = self.status.lock() {
            let state = states
                .entry((peer.space_id.clone(), certificate_id(&peer.certificate)))
                .or_default();
            match result {
                Ok(()) => {
                    state.last_success = Some(chrono::Utc::now().timestamp_millis());
                    state.error = None;
                }
                Err(error) => state.error = Some(error.to_string()),
            }
        }
    }

    fn changed(&self, space_id: &str) {
        if let Some(app) = &self.app {
            let _ = app.emit("sync-space-updated", space_id);
        }
    }
}

async fn bind(port: u16) -> Result<TcpListener, AppError> {
    match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            TcpListener::bind(("0.0.0.0", 0)).await.map_err(io_error)
        }
        Err(error) => Err(io_error(error)),
    }
}

fn blocking_socket(socket: tokio::net::TcpStream) -> Result<TcpStream, AppError> {
    let socket = socket.into_std().map_err(io_error)?;
    socket.set_nonblocking(false).map_err(io_error)?;
    Ok(socket)
}

async fn open_identity(directory: PathBuf) -> Result<Identity, AppError> {
    tokio::task::spawn_blocking(move || {
        Identity::with_certificates(&directory, vec![]).map_err(network_error)
    })
    .await
    .map_err(join_error)?
}

fn parse_invitation(code: &str) -> Result<Invitation, AppError> {
    if code.len() > 8192 {
        return Err(invalid_invitation());
    }
    let encoded = code
        .strip_prefix(INVITATION_PREFIX)
        .ok_or_else(invalid_invitation)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| invalid_invitation())?;
    let invitation: Invitation =
        serde_json::from_slice(&bytes).map_err(|_| invalid_invitation())?;
    if !invitation.name.starts_with("join-")
        || device_name(&invitation.name).is_err()
        || invitation.port == 0
    {
        return Err(invalid_invitation());
    }
    Identity::validate_certificate(&invitation.certificate).map_err(|_| invalid_invitation())?;
    Ok(invitation)
}

fn address_with_port(address: &str, port: u16) -> String {
    if let Ok(ip) = address.parse::<IpAddr>() {
        return SocketAddr::new(ip, port).to_string();
    }
    if address.contains(':') {
        return address.to_owned();
    }
    format!("{address}:{port}")
}

fn device_name(name: &str) -> Result<String, AppError> {
    if name.is_empty()
        || name.len() > 48
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(AppError::InvalidInput("invalid peer device name".into()));
    }
    Ok(name.to_owned())
}

fn certificate_id(certificate: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(certificate)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec(value)
        .map_err(|error| AppError::Internal(format!("encode sync data: {error}")))
}

fn decode<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, AppError> {
    serde_json::from_slice(data)
        .map_err(|error| AppError::InvalidInput(format!("invalid sync data: {error}")))
}

fn invalid_invitation() -> AppError {
    AppError::InvalidInput("invalid Plinth invitation".into())
}

fn state_error() -> AppError {
    AppError::Internal("sync state is unavailable".into())
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::Io(format!("sync connection: {error}"))
}
fn network_error(error: Box<dyn std::error::Error + Send + Sync>) -> AppError {
    AppError::Io(format!("sync connection: {error}"))
}
fn join_error(error: tokio::task::JoinError) -> AppError {
    AppError::Internal(format!("sync task: {error}"))
}
