use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    num::NonZeroU16,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use plinth_sync::{
    PairedDevice, Session as PeerSession,
    consensus::PairingHistory,
    discovery,
    membership::{DeviceId, PeerContact, Role, certificate_fingerprint},
    transport::{self, Identity},
};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tauri::State;
use tokio::{
    net::TcpListener,
    sync::{Semaphore, oneshot},
    task::JoinHandle,
};

use crate::{
    Session,
    commands::{
        spaces::{self, SyncSeed},
        users::{JoinProfile, User, create_join_user},
    },
    db::DbPool,
    error::AppError,
    sync_preview::{PeerConnection, SyncPreview, resolve_peer},
};

const INVITATION_TTL: Duration = Duration::from_secs(300);
const SYNC_INTERVAL: Duration = Duration::from_secs(5);
const SYNC_PORT: u16 = 37419;
const INVITE_PORT: u16 = 37420;
const CONFIRMATION_TTL: Duration = Duration::from_secs(90);
const MAX_CODE_ATTEMPTS: u8 = 5;

#[derive(Serialize, Deserialize)]
struct JoiningDevice {
    label: String,
    #[serde(default)]
    supports_permissions: bool,
    #[serde(default)]
    sync_port: Option<NonZeroU16>,
}

#[derive(Serialize, Deserialize)]
struct Bootstrap {
    seed: SyncSeed,
    port: u16,
    profile: Option<SharedProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permissions: Option<PairingHistory>,
}

#[derive(Serialize, Deserialize)]
struct SharedProfile {
    id: String,
    name: String,
}

#[derive(Serialize, Deserialize)]
struct PairingHost {
    label: String,
    same_user: bool,
}

struct ActiveInvitation {
    id: String,
    owner: String,
    task: JoinHandle<()>,
}

struct Offer {
    invitation_id: String,
    space_id: String,
    user_id: String,
    same_user: bool,
    deadline: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInvitation {
    pub id: String,
    pub name: String,
    pub port: u16,
    pub addresses: Vec<SocketAddr>,
    pub nearby_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPairing {
    pub id: String,
    pub device: String,
    pub code: String,
    pub same_user: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinPairing {
    pub id: String,
    pub device: String,
    pub same_user: bool,
}

struct IncomingPairing {
    invitation_id: String,
    owner: String,
    request: SyncPairing,
    decision: oneshot::Sender<bool>,
}

trait EnrollmentIo: Read + Write + Send {}
impl<T: Read + Write + Send> EnrollmentIo for T {}

struct OutgoingPairing {
    request: JoinPairing,
    code: Option<String>,
    failed_codes: u8,
    stream: Box<dyn EnrollmentIo>,
    certificate: Vec<u8>,
    ip: IpAddr,
    deadline: Instant,
}

enum JoinUser {
    Existing(String),
    New(JoinProfile, Option<String>),
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
    label: String,
    device_id: DeviceId,
    permission_writes: Arc<tokio::sync::Mutex<()>>,
    port: u16,
    invite: Mutex<Option<ActiveInvitation>>,
    incoming: Mutex<Option<IncomingPairing>>,
    outgoing: Mutex<Option<OutgoingPairing>>,
    status: Mutex<std::collections::HashMap<(String, String), DeviceStatus>>,
}

impl SyncNetwork {
    pub(crate) async fn start(
        directory: &Path,
        db: DbPool,
        store: SyncPreview,
    ) -> Result<Self, AppError> {
        let (inner, listener) = Network::open(directory, SYNC_PORT).await?;
        let workers = [
            tokio::spawn(inner.clone().listen(listener, db.clone(), store.clone())),
            tokio::spawn(inner.clone().sync_loop(db, store)),
        ];
        Ok(Self { inner, workers })
    }

    pub(crate) fn stop(&self) {
        self.workers.iter().for_each(JoinHandle::abort);
        if let Ok(mut invitation) = self.inner.invite.lock()
            && let Some(invitation) = invitation.take()
        {
            invitation.task.abort();
        }
    }
}

impl Network {
    async fn open(directory: &Path, port: u16) -> Result<(Arc<Self>, TcpListener), AppError> {
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
        let name = device_name(identity.certificate());
        let listener = bind(port).await?;
        let port = listener.local_addr().map_err(io_error)?.port();
        let inner = Arc::new(Self {
            directory,
            label: local_device_label(&name),
            device_id: certificate_fingerprint(identity.certificate()),
            permission_writes: Arc::default(),
            name,
            port,
            invite: Mutex::new(None),
            incoming: Mutex::new(None),
            outgoing: Mutex::new(None),
            status: Mutex::default(),
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
    same_user: bool,
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
            same_user,
        )
        .await
}

#[tauri::command]
pub async fn begin_sync_pairing(
    address: String,
    candidates: Option<Vec<SocketAddr>>,
    network: State<'_, SyncNetwork>,
) -> Result<JoinPairing, AppError> {
    network
        .inner
        .begin_join(address.trim(), candidates.unwrap_or_default())
        .await
}

#[tauri::command]
pub async fn verify_sync_pairing_code(
    request_id: String,
    code: String,
    network: State<'_, SyncNetwork>,
) -> Result<(), AppError> {
    network.inner.verify_join_code(&request_id, &code)
}

#[tauri::command]
pub async fn cancel_sync_invitation(
    invitation_id: String,
    session: State<'_, Session>,
    network: State<'_, SyncNetwork>,
) -> Result<(), AppError> {
    let user = session.require_user()?;
    let mut active = network.inner.invite.lock().map_err(|_| state_error())?;
    if active
        .as_ref()
        .is_some_and(|invite| invite.id == invitation_id && invite.owner == user.user_id)
    {
        if let Some(invite) = active.take() {
            invite.task.abort();
        }
        let mut incoming = network.inner.incoming.lock().map_err(|_| state_error())?;
        incoming.take_if(|request| request.invitation_id == invitation_id);
    }
    Ok(())
}

#[tauri::command]
pub async fn sync_invitation_request(
    invitation_id: String,
    session: State<'_, Session>,
    network: State<'_, SyncNetwork>,
) -> Result<Option<SyncPairing>, AppError> {
    let user = session.require_user()?;
    Ok(network
        .inner
        .incoming
        .lock()
        .map_err(|_| state_error())?
        .as_ref()
        .filter(|request| request.invitation_id == invitation_id && request.owner == user.user_id)
        .map(|request| request.request.clone()))
}

#[tauri::command]
pub async fn respond_sync_pairing(
    request_id: String,
    approved: bool,
    session: State<'_, Session>,
    network: State<'_, SyncNetwork>,
) -> Result<(), AppError> {
    let user = session.require_user()?;
    network
        .inner
        .respond_pairing(&user.user_id, &request_id, approved)
}

#[tauri::command]
pub async fn cancel_sync_pairing(
    request_id: String,
    network: State<'_, SyncNetwork>,
) -> Result<(), AppError> {
    let mut pending = network.inner.outgoing.lock().map_err(|_| state_error())?;
    if pending
        .as_ref()
        .is_some_and(|pair| pair.request.id == request_id)
    {
        pending.take();
    }
    Ok(())
}

#[tauri::command]
pub async fn join_shared_space(
    request_id: String,
    profile: Option<JoinProfile>,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    store: State<'_, SyncPreview>,
    network: State<'_, SyncNetwork>,
) -> Result<String, AppError> {
    let user = match (session.require_user(), profile) {
        (Ok(user), None) => JoinUser::Existing(user.user_id),
        (Err(AppError::Unauthorized), Some(profile)) => JoinUser::New(profile, None),
        (Ok(user), Some(profile)) => JoinUser::New(profile, Some(user.user_id)),
        _ => return Err(AppError::Unauthorized),
    };
    let (id, new_user) = network
        .inner
        .join(db.inner(), store.inner(), user, &request_id)
        .await?;
    if let Some(user_id) = new_user {
        session.set(user_id, Some(id.clone()))?;
    }
    Ok(id)
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
                name: peer.display_name.unwrap_or_else(|| {
                    peer.address
                        .as_deref()
                        .and_then(|value| value.parse::<SocketAddr>().ok())
                        .map_or_else(
                            || device_label_from_id(&peer.name),
                            |addr| format!("Device at {}", addr.ip()),
                        )
                }),
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
        same_user: bool,
    ) -> Result<SyncInvitation, AppError> {
        store
            .membership_log(space_id.clone())
            .await?
            .owner_can_pair(&user_id, self.device_id)
            .await
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;
        if let Some(previous) = self.invite.lock().map_err(|_| state_error())?.take() {
            previous.task.abort();
        }
        self.incoming.lock().map_err(|_| state_error())?.take();
        let listener = bind(INVITE_PORT).await?;
        let address = listener.local_addr().map_err(io_error)?;
        let addresses = discovery::lan_addresses(address.port()).unwrap_or_default();
        let id = format!("join-{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
        let label = self.label.clone();
        let advertisement = discovery::advertise_managed(&id, address, &label).ok();
        let nearby_available = advertisement.is_some();
        let invitation_id = id.clone();
        let owner = user_id.clone();
        let network = self.clone();
        let task = tokio::spawn(async move {
            let _advertisement = advertisement;
            let offer = Offer {
                invitation_id,
                space_id,
                user_id,
                same_user,
                deadline: Instant::now() + INVITATION_TTL,
            };
            while let Some(remaining) = offer.deadline.checked_duration_since(Instant::now()) {
                let Ok(Ok((socket, _))) = tokio::time::timeout(remaining, listener.accept()).await
                else {
                    break;
                };
                if let Ok(socket) = blocking_socket(socket)
                    && let Ok(true) = network.enroll(&db, &store, &offer, socket).await
                {
                    break;
                }
            }
        });
        let mut guard = self.invite.lock().map_err(|_| state_error())?;
        if let Some(previous) = guard.replace(ActiveInvitation {
            id: id.clone(),
            owner,
            task,
        }) {
            previous.task.abort();
        }
        Ok(SyncInvitation {
            id,
            name: label,
            port: address.port(),
            addresses,
            nearby_available,
        })
    }

    fn respond_pairing(&self, owner: &str, id: &str, approved: bool) -> Result<(), AppError> {
        let mut request = self.incoming.lock().map_err(|_| state_error())?;
        if request
            .as_ref()
            .is_none_or(|request| request.owner != owner || request.request.id != id)
        {
            return Err(AppError::NotFound("pairing request has ended".into()));
        }
        let request = request.take().ok_or_else(state_error)?;
        request
            .decision
            .send(approved)
            .map_err(|_| AppError::NotFound("pairing request has ended".into()))
    }

    async fn enroll(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        offer: &Offer,
        socket: TcpStream,
    ) -> Result<bool, AppError> {
        let directory = self.directory.clone();
        let remote_addr = socket.peer_addr().map_err(io_error)?;
        let mut log = store.membership_log(offer.space_id.clone()).await?;
        let managed = log
            .owner_can_pair(&offer.user_id, self.device_id)
            .await
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;
        let host = encode(&PairingHost {
            label: self.label.clone(),
            same_user: offer.same_user,
        })?;
        let (mut stream, certificate, request, code) = tokio::task::spawn_blocking(move || {
            let identity =
                Identity::with_certificates(&directory, vec![]).map_err(network_error)?;
            let (mut stream, certificate, binding) =
                identity.accept_enrollment(socket).map_err(network_error)?;
            transport::write_frame(&mut stream, &host, transport::MAX_ENROLLMENT_REQUEST)
                .map_err(io_error)?;
            let request: JoiningDevice = decode(
                &transport::read_frame(&mut stream, transport::MAX_ENROLLMENT_REQUEST)
                    .map_err(io_error)?,
            )?;
            let code = pairing_code(&mut stream, &binding, true)?;
            Ok::<_, AppError>((stream, certificate, request, code))
        })
        .await
        .map_err(join_error)??;
        if managed && !request.supports_permissions {
            return Err(AppError::InvalidInput(
                "Update Plinth on the joining device before pairing.".into(),
            ));
        }
        let timeout = offer
            .deadline
            .saturating_duration_since(Instant::now())
            .min(CONFIRMATION_TTL);
        if timeout.is_zero() {
            return Ok(false);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let label = discovery::usable_device_label(&request.label)
            .unwrap_or_else(|| format!("Device at {}", remote_addr.ip()));
        let (send, receive) = oneshot::channel();
        *self.incoming.lock().map_err(|_| state_error())? = Some(IncomingPairing {
            invitation_id: offer.invitation_id.clone(),
            owner: offer.user_id.clone(),
            request: SyncPairing {
                id: id.clone(),
                device: label.clone(),
                code,
                same_user: offer.same_user,
            },
            decision: send,
        });
        let approved = matches!(tokio::time::timeout(timeout, receive).await, Ok(Ok(true)));
        {
            let mut pending = self.incoming.lock().map_err(|_| state_error())?;
            if pending
                .as_ref()
                .is_some_and(|request| request.request.id == id)
            {
                pending.take();
            }
        }
        let (mut stream, ready) = tokio::task::spawn_blocking(move || {
            transport::write_frame(&mut stream, &[u8::from(approved)], 1).map_err(io_error)?;
            let ready = approved && transport::read_frame(&mut stream, 1).map_err(io_error)? == [1];
            Ok::<_, AppError>((stream, ready))
        })
        .await
        .map_err(join_error)??;
        if !ready {
            return Ok(false);
        }
        let (mut snapshot, seed) =
            spaces::prepare_sync_seed_export(db, &offer.space_id, &offer.user_id).await?;
        let profile = if offer.same_user {
            let user = sqlx::query_file_as!(User, "queries/users/get_user.sql", offer.user_id)
                .fetch_one(&mut *snapshot)
                .await?;
            Some(SharedProfile {
                id: user.id,
                name: user.name,
            })
        } else {
            None
        };
        let directory = self.directory.clone();
        let owner = offer.user_id.clone();
        let device_id = self.device_id;
        let joining_contact = PeerContact {
            certificate: certificate.clone(),
            address_hint: request
                .sync_port
                .map(|port| SocketAddr::new(remote_addr.ip(), port.get())),
        };
        let role = if offer.same_user {
            Role::Owner
        } else {
            Role::Member
        };
        let permission_writes = self.permission_writes.clone();
        // Complete the committed write even if the invitation closes mid-approval.
        let permissions = tokio::spawn(async move {
            let _guard = permission_writes.lock().await;
            log.approve_pairing(directory, &owner, device_id, joining_contact, role)
                .await
        })
        .await
        .map_err(join_error)?
        .map_err(network_error)?;
        let data = encode(&Bootstrap {
            seed,
            port: self.port,
            profile,
            permissions,
        })?;
        if data.len() > transport::MAX_SPACE_SNAPSHOT {
            return Err(AppError::InvalidInput(
                "space snapshot exceeds the transfer limit".into(),
            ));
        }
        let peer = PairedDevice {
            space_id: offer.space_id.clone(),
            name: device_name(&certificate),
            certificate,
            address: None,
            display_name: Some(label),
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
        Ok(true)
    }

    async fn begin_join(
        &self,
        address: &str,
        mut candidates: Vec<SocketAddr>,
    ) -> Result<JoinPairing, AppError> {
        if address.is_empty() {
            return Err(AppError::InvalidInput("select an inviting device".into()));
        }
        if !address.starts_with("mdns:") || candidates.is_empty() {
            candidates = resolve_peer(address_with_port(address, INVITE_PORT)).await?;
        } else {
            candidates.retain(|addr| addr.port() != 0 && !addr.ip().is_unspecified());
            candidates.truncate(16);
        }
        let identity = open_identity(self.directory.clone()).await?;
        let request = encode(&JoiningDevice {
            label: self.label.clone(),
            supports_permissions: true,
            sync_port: NonZeroU16::new(self.port),
        })?;
        let pending = tokio::task::spawn_blocking(move || {
            let mut error = AppError::Io("no pairing address available".into());
            for address in candidates {
                match identity.connect_enrollment(address) {
                    Ok((mut stream, certificate, binding)) => {
                        let label =
                            transport::read_frame(&mut stream, transport::MAX_ENROLLMENT_REQUEST)
                                .map_err(io_error)?;
                        let host: PairingHost = decode(&label)?;
                        transport::write_frame(
                            &mut stream,
                            &request,
                            transport::MAX_ENROLLMENT_REQUEST,
                        )
                        .map_err(io_error)?;
                        let code = pairing_code(&mut stream, &binding, false)?;
                        let request = JoinPairing {
                            id: uuid::Uuid::new_v4().to_string(),
                            device: discovery::usable_device_label(&host.label)
                                .unwrap_or_else(|| format!("Device at {}", address.ip())),
                            same_user: host.same_user,
                        };
                        return Ok(OutgoingPairing {
                            request,
                            code: Some(code),
                            failed_codes: 0,
                            stream: Box::new(stream),
                            certificate,
                            ip: address.ip(),
                            deadline: Instant::now() + CONFIRMATION_TTL,
                        });
                    }
                    Err(failure) => error = network_error(failure),
                }
            }
            Err(error)
        })
        .await
        .map_err(join_error)??;
        let request = pending.request.clone();
        *self.outgoing.lock().map_err(|_| state_error())? = Some(pending);
        Ok(request)
    }

    fn verify_join_code(&self, request_id: &str, code: &str) -> Result<(), AppError> {
        let mut slot = self.outgoing.lock().map_err(|_| state_error())?;
        let pending = slot
            .as_mut()
            .filter(|pending| pending.request.id == request_id && Instant::now() < pending.deadline)
            .ok_or_else(|| AppError::NotFound("pairing request has ended".into()))?;
        let Some(expected) = pending.code.as_ref() else {
            return Ok(());
        };
        if bool::from(expected.as_bytes().ct_eq(code.as_bytes())) {
            pending.code = None;
            return Ok(());
        }
        pending.failed_codes += 1;
        if pending.failed_codes >= MAX_CODE_ATTEMPTS {
            slot.take();
            return Err(AppError::InvalidInput(
                "Too many incorrect codes. Ask the host to reject this request, then try again."
                    .into(),
            ));
        }
        Err(AppError::InvalidInput("Incorrect joining code.".into()))
    }

    async fn join(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        user: JoinUser,
        request_id: &str,
    ) -> Result<(String, Option<String>), AppError> {
        let pending = {
            let mut slot = self.outgoing.lock().map_err(|_| state_error())?;
            if slot
                .as_ref()
                .is_none_or(|pending| pending.request.id != request_id)
            {
                return Err(AppError::NotFound("pairing request has ended".into()));
            }
            let pending = slot.as_ref().ok_or_else(state_error)?;
            if pending.code.is_some() {
                return Err(AppError::InvalidInput(
                    "Enter the joining code first.".into(),
                ));
            }
            if let JoinUser::New(profile, _) = &user {
                profile.validate(pending.request.same_user)?;
            }
            slot.take().ok_or_else(state_error)?
        };
        if Instant::now() >= pending.deadline {
            return Err(AppError::NotFound("pairing request has ended".into()));
        }
        let (ip, bootstrap, certificate, label) = tokio::task::spawn_blocking(move || {
            let OutgoingPairing {
                request,
                mut stream,
                ip,
                certificate,
                ..
            } = pending;
            // Both sides confirm before the source sends any finance data.
            transport::write_frame(&mut stream, &[1], 1).map_err(io_error)?;
            if transport::read_frame(&mut stream, 1).map_err(io_error)? != [1] {
                return Err(AppError::InvalidInput(
                    "The invitation was declined or expired.".into(),
                ));
            }
            let data = transport::read_frame(&mut stream, transport::MAX_SPACE_SNAPSHOT)
                .map_err(io_error)?;
            let bootstrap: Bootstrap = decode(&data)?;
            if bootstrap.profile.is_some() != request.same_user {
                return Err(AppError::InvalidInput(
                    "pairing account changed during the invitation".into(),
                ));
            }
            Ok((ip, bootstrap, certificate, request.device))
        })
        .await
        .map_err(join_error)??;
        if bootstrap.port == 0 {
            return Err(AppError::InvalidInput("missing peer sync port".into()));
        }
        let space_id = bootstrap.seed.space_id.clone();
        let host = certificate_fingerprint(&certificate);
        let peer = PairedDevice {
            space_id: space_id.clone(),
            name: device_name(&certificate),
            certificate,
            address: Some(SocketAddr::new(ip, bootstrap.port).to_string()),
            display_name: Some(label),
        };
        let mut install = db.begin_with("BEGIN IMMEDIATE").await?;
        let shared_account = bootstrap.profile.as_ref().map(|profile| profile.id.clone());
        let (user_id, new) = match (user, bootstrap.profile) {
            (JoinUser::Existing(id), None) => (id, false),
            (JoinUser::Existing(id), Some(profile)) if id == profile.id => (id, false),
            (JoinUser::New(_, Some(current)), Some(profile)) if current == profile.id => {
                (current, false)
            }
            (JoinUser::New(profile, current), Some(identity)) => {
                let id = create_join_user(
                    &mut install,
                    profile,
                    Some((identity.id, identity.name)),
                    current.is_none(),
                )
                .await?;
                (id, true)
            }
            (JoinUser::New(profile, None), None) => (
                create_join_user(&mut install, profile, None, true).await?,
                true,
            ),
            _ => return Err(AppError::Unauthorized),
        };
        spaces::write_sync_seed(&mut install, &user_id, bootstrap.seed).await?;
        store
            .membership_log(space_id.clone())
            .await?
            .receive_pairing_history(
                bootstrap.permissions,
                host,
                self.device_id,
                shared_account.as_deref(),
            )
            .await
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;
        store
            .with_store(move |store| store.pair_device(&peer))
            .await?;
        install.commit().await?;
        Ok((space_id, new.then_some(user_id)))
    }

    async fn listen(self: Arc<Self>, listener: TcpListener, db: DbPool, store: SyncPreview) {
        let _advertisement = listener.local_addr().ok().and_then(|address| {
            discovery::advertise_managed(&self.name, address, &self.label).ok()
        });
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
        let result = tokio::task::spawn_blocking(move || {
            let mut session = PeerSession::new(&space);
            transport::respond(&mut stream, &space, |incoming| {
                let (next, outgoing) = runtime.block_on(store.exchange_step(
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
        let mut result = self.dial(db, store, &peer, address.clone()).await;
        if result.is_err() {
            result = self
                .dial(db, store, &peer, format!("mdns:{}", peer.name))
                .await;
        }
        self.record_status(&peer, &result);
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

fn local_device_label(fallback: &str) -> String {
    [whoami::devicename(), whoami::hostname()]
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|name| discovery::usable_device_label(&name))
        .or_else(|| {
            discovery::lan_addresses(SYNC_PORT)
                .ok()?
                .first()
                .map(|addr| format!("Device at {}", addr.ip()))
        })
        .unwrap_or_else(|| device_label_from_id(fallback))
}

fn device_label_from_id(name: &str) -> String {
    name.strip_prefix("plinth-")
        .map_or_else(|| name.to_owned(), |id| format!("Plinth device {id}"))
}

fn pairing_code(
    stream: &mut (impl Read + Write),
    binding: &[u8; 32],
    source: bool,
) -> Result<String, AppError> {
    use sha2::{Digest, Sha256};
    let commitment = |nonce: &[u8], from_source: bool| {
        let mut hash = Sha256::new();
        hash.update(b"plinth pairing commitment");
        hash.update(binding);
        hash.update([u8::from(from_source)]);
        hash.update(nonce);
        hash.finalize()
    };
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|error| AppError::Internal(error.to_string()))?;
    transport::write_frame(stream, &commitment(&nonce, source), 32).map_err(io_error)?;
    let remote_commitment = transport::read_frame(stream, 32).map_err(io_error)?;
    transport::write_frame(stream, &nonce, 32).map_err(io_error)?;
    let remote_nonce = transport::read_frame(stream, 32).map_err(io_error)?;
    if remote_nonce.len() != 32
        || !bool::from(
            commitment(&remote_nonce, !source)
                .as_slice()
                .ct_eq(&remote_commitment),
        )
    {
        return Err(AppError::Forbidden);
    }
    // Both nonces are fixed before either is revealed.
    let mut hash = Sha256::new();
    hash.update(b"plinth pairing code");
    hash.update(binding);
    if source {
        hash.update(nonce);
        hash.update(remote_nonce);
    } else {
        hash.update(remote_nonce);
        hash.update(nonce);
    }
    let digest = hash.finalize();
    let number = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    Ok(format!("{:06}", number % 1_000_000))
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

fn device_name(certificate: &[u8]) -> String {
    format!("plinth-{}", &certificate_id(certificate)[..12])
}

fn certificate_id(certificate: &[u8]) -> String {
    certificate_fingerprint(certificate)
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
