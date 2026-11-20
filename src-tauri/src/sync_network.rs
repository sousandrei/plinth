use std::{
    collections::{BTreeSet, HashMap},
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    num::NonZeroU16,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use plinth_sync::{
    PairedDevice, Session as PeerSession,
    consensus::{PairingHistory, PermissionReplica},
    discovery,
    membership::{DeviceId, Enrollment, Membership, PeerContact, Role, certificate_fingerprint},
    transport::{self, Identity, PeerProtocol},
};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::{net::TcpListener, sync::Semaphore, task::JoinHandle};

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

#[path = "sync_pairing_code.rs"]
mod pairing_code;

const INVITATION_TTL: Duration = Duration::from_secs(300);
const SYNC_INTERVAL: Duration = Duration::from_secs(5);
const SYNC_PORT: u16 = 37419;
const INVITE_PORT: u16 = 37420;
const CONFIRMATION_TTL: Duration = Duration::from_secs(90);
const MAX_CODE_ATTEMPTS: u8 = 5;
const MAX_ROSTER_BYTES: usize = 8 * 1024 * 1024;

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
    remaining_attempts: Arc<AtomicU8>,
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
    pub code: Option<String>,
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
}

trait EnrollmentIo: Read + Write + Send {}
impl<T: Read + Write + Send> EnrollmentIo for T {}

struct OutgoingPairing {
    request: JoinPairing,
    verified: bool,
    binding: [u8; 32],
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
    permission_writes: tokio::sync::Mutex<()>,
    permission_calls: tokio::sync::RwLock<()>,
    permission_replicas: tokio::sync::Mutex<Option<HashMap<String, Arc<PermissionReplica>>>>,
    port: u16,
    invite: Mutex<Option<ActiveInvitation>>,
    incoming: Mutex<Option<IncomingPairing>>,
    outgoing: tokio::sync::Mutex<Option<OutgoingPairing>>,
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

    pub(crate) async fn stop(&self) -> Result<(), AppError> {
        self.workers.iter().for_each(JoinHandle::abort);
        if let Ok(mut invitation) = self.inner.invite.lock()
            && let Some(invitation) = invitation.take()
        {
            invitation.task.abort();
        }
        self.inner.shutdown_permissions().await
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
            permission_writes: tokio::sync::Mutex::default(),
            permission_calls: tokio::sync::RwLock::default(),
            permission_replicas: tokio::sync::Mutex::new(Some(HashMap::new())),
            name,
            port,
            invite: Mutex::new(None),
            incoming: Mutex::new(None),
            outgoing: tokio::sync::Mutex::new(None),
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
    network.inner.verify_join_code(&request_id, &code).await
}

#[tauri::command]
pub async fn cancel_sync_invitation(
    invitation_id: String,
    session: State<'_, Session>,
    network: State<'_, SyncNetwork>,
) -> Result<(), AppError> {
    let user = session.require_user()?;
    network
        .inner
        .cancel_invitation(&invitation_id, &user.user_id)
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
pub async fn cancel_sync_pairing(
    request_id: String,
    network: State<'_, SyncNetwork>,
) -> Result<(), AppError> {
    network.inner.cancel_join(&request_id).await;
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
    fn cancel_invitation(&self, invitation_id: &str, user_id: &str) -> Result<(), AppError> {
        let mut active = self.invite.lock().map_err(|_| state_error())?;
        if active
            .as_ref()
            .is_some_and(|invite| invite.id == invitation_id && invite.owner == user_id)
        {
            let mut incoming = self.incoming.lock().map_err(|_| state_error())?;
            if incoming.as_ref().is_some_and(|request| {
                request.invitation_id == invitation_id && request.request.code.is_none()
            }) {
                return Ok(());
            }
            if let Some(invite) = active.take() {
                invite.task.abort();
            }
            incoming.take_if(|request| request.invitation_id == invitation_id);
        }
        Ok(())
    }

    async fn cancel_join(&self, request_id: &str) {
        self.outgoing
            .lock()
            .await
            .take_if(|pair| pair.request.id == request_id);
    }

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
                remaining_attempts: Arc::new(AtomicU8::new(MAX_CODE_ATTEMPTS)),
            };
            while let Some(remaining) = offer.deadline.checked_duration_since(Instant::now()) {
                if offer.remaining_attempts.load(Ordering::Relaxed) == 0 {
                    break;
                }
                let Ok(Ok((socket, _))) = tokio::time::timeout(remaining, listener.accept()).await
                else {
                    break;
                };
                if let Ok(socket) = blocking_socket(socket) {
                    let joined =
                        matches!(network.enroll(&db, &store, &offer, socket).await, Ok(true));
                    let verified = network.incoming.lock().is_ok_and(|incoming| {
                        incoming.as_ref().is_some_and(|request| {
                            request.invitation_id == offer.invitation_id
                                && request.request.code.is_none()
                        })
                    });
                    if joined || verified {
                        break;
                    }
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

    async fn enroll(
        self: &Arc<Self>,
        db: &DbPool,
        store: &SyncPreview,
        offer: &Offer,
        socket: TcpStream,
    ) -> Result<bool, AppError> {
        let directory = self.directory.clone();
        let remote_addr = socket.peer_addr().map_err(io_error)?;
        let managed = store
            .membership_log(offer.space_id.clone())
            .await?
            .owner_can_pair(&offer.user_id, self.device_id)
            .await
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;
        let host = encode(&PairingHost {
            label: self.label.clone(),
            same_user: offer.same_user,
        })?;
        let (mut stream, certificate, request, binding) = tokio::task::spawn_blocking(move || {
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
            Ok::<_, AppError>((stream, certificate, request, binding))
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
        let code =
            pairing_code::generate().map_err(|error| AppError::Internal(error.to_string()))?;
        let deadline = Instant::now() + timeout;
        *self.incoming.lock().map_err(|_| state_error())? = Some(IncomingPairing {
            invitation_id: offer.invitation_id.clone(),
            owner: offer.user_id.clone(),
            request: SyncPairing {
                id: id.clone(),
                device: label.clone(),
                code: Some(code.clone()),
                same_user: offer.same_user,
            },
        });
        let remaining_attempts = offer.remaining_attempts.clone();
        let verified = tokio::task::spawn_blocking(move || {
            while remaining_attempts.load(Ordering::Relaxed) != 0 {
                let proof =
                    pairing_code::host_proof(&mut stream, &code, &binding).map_err(io_error)?;
                if Instant::now() >= deadline {
                    break;
                }
                if let Some(proof) = proof {
                    return Ok::<_, AppError>(Some((stream, proof)));
                }
                let remaining = remaining_attempts.fetch_sub(1, Ordering::Relaxed) - 1;
                let reply: &[u8] = if remaining == 0 { &[0] } else { &[] };
                transport::write_frame(&mut stream, reply, 32).map_err(io_error)?;
            }
            Ok(None)
        })
        .await
        .map_err(join_error)?;
        {
            let mut pending = self.incoming.lock().map_err(|_| state_error())?;
            let Some(request) = pending.as_mut().filter(|request| request.request.id == id) else {
                return Ok(false);
            };
            if matches!(verified, Ok(Some(_))) {
                request.request.code = None;
            } else {
                *pending = None;
            }
        }
        let Some((mut stream, proof)) = verified? else {
            return Ok(false);
        };
        let (mut stream, ready) = tokio::task::spawn_blocking(move || {
            transport::write_frame(&mut stream, proof.as_ref(), 32).map_err(io_error)?;
            let ready = transport::read_frame(&mut stream, 1).map_err(io_error)? == [1];
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
        let owner = offer.user_id.clone();
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
        let network = self.clone();
        let preview = store.clone();
        let space = offer.space_id.clone();
        // Complete the committed write even if the invitation closes mid-approval.
        let permissions = tokio::spawn(async move {
            if !managed {
                return Ok(None);
            }
            let _guard = network.permission_writes.lock().await;
            let replica = network.permission_replica(&preview, &space).await?;
            replica
                .approve_pairing(&owner, joining_contact, role)
                .await
                .map(Some)
                .map_err(network_error)
        })
        .await
        .map_err(join_error)??;
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
                        let request = JoinPairing {
                            id: uuid::Uuid::new_v4().to_string(),
                            device: discovery::usable_device_label(&host.label)
                                .unwrap_or_else(|| format!("Device at {}", address.ip())),
                            same_user: host.same_user,
                        };
                        return Ok(OutgoingPairing {
                            request,
                            verified: false,
                            binding,
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
        *self.outgoing.lock().await = Some(pending);
        Ok(request)
    }

    async fn verify_join_code(&self, request_id: &str, code: &str) -> Result<(), AppError> {
        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(AppError::InvalidInput(
                "Enter the six-digit joining code.".into(),
            ));
        }
        let mut slot = self.outgoing.lock().await;
        let pending = slot
            .as_ref()
            .filter(|pending| pending.request.id == request_id && Instant::now() < pending.deadline)
            .ok_or_else(|| AppError::NotFound("pairing request has ended".into()))?;
        if pending.verified {
            return Ok(());
        }
        let mut pending = slot.take().ok_or_else(state_error)?;
        let code = code.to_owned();
        let (mut pending, verified) = tokio::task::spawn_blocking(move || {
            let verified = pairing_code::verify(&mut pending.stream, &code, &pending.binding)
                .map_err(io_error)?;
            Ok::<_, AppError>((pending, verified))
        })
        .await
        .map_err(join_error)??;
        match verified {
            pairing_code::Verification::Accepted => {
                pending.verified = true;
                pending.deadline = Instant::now() + CONFIRMATION_TTL;
                *slot = Some(pending);
                Ok(())
            }
            pairing_code::Verification::Incorrect => {
                *slot = Some(pending);
                Err(AppError::InvalidInput("Incorrect joining code.".into()))
            }
            pairing_code::Verification::Exhausted => Err(AppError::InvalidInput(
                "Too many incorrect codes. Ask the host for a new invitation.".into(),
            )),
        }
    }

    async fn join(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        user: JoinUser,
        request_id: &str,
    ) -> Result<(String, Option<String>), AppError> {
        let pending = {
            let mut slot = self.outgoing.lock().await;
            if slot
                .as_ref()
                .is_none_or(|pending| pending.request.id != request_id)
            {
                return Err(AppError::NotFound("pairing request has ended".into()));
            }
            let pending = slot.as_ref().ok_or_else(state_error)?;
            if !pending.verified {
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
        let mut membership_log = store.membership_log(space_id.clone()).await?;
        membership_log
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
        if let Some(membership) = membership_log
            .membership()
            .await
            .map_err(network_error_box)?
        {
            self.cache_roster(store, &space_id, &membership).await?;
        }
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

    async fn permission_replica(
        &self,
        store: &SyncPreview,
        space: &str,
    ) -> Result<Arc<PermissionReplica>, AppError> {
        let mut state = self.permission_replicas.lock().await;
        let replicas = state.as_mut().ok_or_else(state_error)?;
        if let Some(replica) = replicas.get(space) {
            return Ok(replica.clone());
        }
        let scope = space.to_owned();
        let addresses: HashMap<_, SocketAddr> = store
            .with_store(move |store| {
                Ok(store
                    .paired_devices()?
                    .into_iter()
                    .filter(|peer| peer.space_id == scope)
                    .filter_map(|peer| {
                        Some((
                            certificate_fingerprint(&peer.certificate),
                            peer.address?.parse().ok()?,
                        ))
                    })
                    .collect())
            })
            .await?;
        let log = store.membership_log(space.to_owned()).await?;
        let replica = Arc::new(
            PermissionReplica::open(log, self.directory.clone(), move |contact| {
                addresses.get(&contact.device_id()).copied()
            })
            .await
            .map_err(network_error)?,
        );
        replicas.insert(space.to_owned(), replica.clone());
        Ok(replica)
    }

    async fn shutdown_permissions(&self) -> Result<(), AppError> {
        let _calls = self.permission_calls.write().await;
        let _writes = self.permission_writes.lock().await;
        let replicas = self
            .permission_replicas
            .lock()
            .await
            .take()
            .unwrap_or_default();
        let mut result = Ok(());
        for replica in replicas.into_values() {
            if let Err(error) = replica.shutdown().await {
                result = Err(network_error(error));
            }
        }
        result
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
        let (mut stream, certificate, space_id, protocol) =
            tokio::task::spawn_blocking(move || {
                let identity = Identity::with_certificates(
                    &directory,
                    certificates.into_iter().map(Into::into).collect(),
                )
                .map_err(network_error)?;
                let (mut stream, certificate, protocol) = identity
                    .desktop_listener()
                    .map_err(network_error)?
                    .accept_protocol(socket)
                    .map_err(network_error)?;
                let space = transport::read_space(&mut stream).map_err(network_error)?;
                Ok::<_, AppError>((stream, certificate, space, protocol))
            })
            .await
            .map_err(join_error)??;
        let (peer, enrollment) = self
            .current_peer(db, store, &space_id, &certificate)
            .await?;
        if protocol == PeerProtocol::Roster {
            return self.serve_roster(store, &peer, stream).await;
        }
        if protocol == PeerProtocol::Permissions {
            let _call = self.permission_calls.read().await;
            let Some(enrollment) = enrollment else {
                return Err(AppError::Forbidden);
            };
            if !store
                .membership_log(space_id.clone())
                .await?
                .is_committed_voter(enrollment.device)
                .await
                .map_err(network_error)?
            {
                return Err(AppError::Forbidden);
            }
            let replica = self.permission_replica(store, &space_id).await?;
            return replica
                .respond(stream, &certificate)
                .await
                .map_err(network_error);
        }
        let db = db.clone();
        let store = store.clone();
        let runtime = tokio::runtime::Handle::current();
        let space = space_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut session = PeerSession::new(&space).with_peer(enrollment);
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
    ) -> Result<(PairedDevice, Option<Enrollment>), AppError> {
        let exists = sqlx::query_file!("queries/spaces/get_space_name.sql", space_id)
            .fetch_optional(db)
            .await?
            .is_some();
        if !exists {
            return Err(AppError::Forbidden);
        }
        let membership = store
            .membership_log(space_id.to_owned())
            .await?
            .membership()
            .await
            .map_err(network_error_box)?;
        let enrollment = if let Some(membership) = membership {
            let device = certificate_fingerprint(certificate);
            if !membership.permits_sync(self.device_id) || !membership.permits_sync(device) {
                return Err(AppError::Forbidden);
            }
            Some(
                membership
                    .device(device)
                    .ok_or(AppError::Forbidden)?
                    .enrollment,
            )
        } else {
            None
        };
        store
            .with_store(|store| store.paired_devices())
            .await?
            .into_iter()
            .find(|peer| peer.space_id == space_id && peer.certificate == certificate)
            .map(|peer| (peer, enrollment))
            .ok_or(AppError::Forbidden)
    }

    async fn sync_loop(self: Arc<Self>, db: DbPool, store: SyncPreview) {
        loop {
            if let Ok(peers) = store.with_store(|store| store.paired_devices()).await {
                let spaces: BTreeSet<_> = peers.iter().map(|peer| peer.space_id.as_str()).collect();
                for space in spaces {
                    if let Err(error) = self.resume_permission_replica(&db, &store, space).await {
                        eprintln!("resume permission replica for {space}: {error}");
                    }
                }
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

    async fn resume_permission_replica(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        space: &str,
    ) -> Result<(), AppError> {
        if self
            .permission_replicas
            .lock()
            .await
            .as_ref()
            .is_none_or(|replicas| replicas.contains_key(space))
        {
            return Ok(());
        }
        if sqlx::query_file!("queries/spaces/get_space_name.sql", space)
            .fetch_optional(db)
            .await?
            .is_none()
        {
            return Ok(());
        }
        let log = store.membership_log(space.to_owned()).await?;
        if log
            .should_resume_replica(self.device_id)
            .await
            .map_err(network_error)?
        {
            self.permission_replica(store, space).await?;
        }
        Ok(())
    }

    async fn sync_peer(&self, db: &DbPool, store: &SyncPreview, peer: PairedDevice) {
        let Ok((peer, enrollment)) = self
            .current_peer(db, store, &peer.space_id, &peer.certificate)
            .await
        else {
            return;
        };
        let Some(mut address) = peer.address.clone() else {
            return;
        };
        let mut result = self
            .dial(db, store, &peer, address.clone(), enrollment)
            .await;
        let nearby = format!("mdns:{}", peer.name);
        if result.is_err() && address != nearby {
            address = nearby;
            result = self
                .dial(db, store, &peer, address.clone(), enrollment)
                .await;
        }
        if result.is_ok() {
            let _ = self.refresh_roster(store, &peer, address).await;
        }
        self.record_status(&peer, &result);
    }

    async fn serve_roster(
        &self,
        store: &SyncPreview,
        peer: &PairedDevice,
        mut stream: impl Read + Write + Send + 'static,
    ) -> Result<(), AppError> {
        let (mut stream, revision) = tokio::task::spawn_blocking(move || {
            let revision: [u8; 8] = transport::read_frame(&mut stream, 8)
                .map_err(io_error)?
                .try_into()
                .map_err(|_| AppError::InvalidInput("invalid roster revision".into()))?;
            Ok::<_, AppError>((stream, u64::from_be_bytes(revision)))
        })
        .await
        .map_err(join_error)??;
        let update = store
            .membership_log(peer.space_id.clone())
            .await?
            .roster_update(
                self.device_id,
                certificate_fingerprint(&peer.certificate),
                revision,
            )
            .await
            .map_err(network_error)?;
        let bytes = update.as_ref().map(encode).transpose()?.unwrap_or_default();
        tokio::task::spawn_blocking(move || {
            transport::write_frame(&mut stream, &bytes, MAX_ROSTER_BYTES).map_err(io_error)
        })
        .await
        .map_err(join_error)?
    }

    async fn refresh_roster(
        &self,
        store: &SyncPreview,
        peer: &PairedDevice,
        address: String,
    ) -> Result<(), AppError> {
        let mut log = store.membership_log(peer.space_id.clone()).await?;
        let host = certificate_fingerprint(&peer.certificate);
        let Some(revision) = log
            .roster_revision(host, self.device_id)
            .await
            .map_err(network_error)?
        else {
            return Ok(());
        };
        let directory = self.directory.clone();
        let certificate = peer.certificate.clone();
        let space = peer.space_id.clone();
        let addresses = resolve_peer(address).await?;
        let update = tokio::task::spawn_blocking(move || {
            let identity = Identity::with_certificates(&directory, vec![certificate.into()])
                .map_err(network_error)?;
            let mut stream = addresses
                .into_iter()
                .find_map(|address| identity.connect_roster(address).ok())
                .ok_or_else(|| AppError::Io("could not connect to the space roster".into()))?;
            transport::write_frame(&mut stream, space.as_bytes(), 256).map_err(io_error)?;
            transport::write_frame(&mut stream, &revision.to_be_bytes(), 8).map_err(io_error)?;
            let bytes = transport::read_frame(&mut stream, MAX_ROSTER_BYTES).map_err(io_error)?;
            if bytes.is_empty() {
                return Ok(None);
            }
            decode::<PairingHistory>(&bytes).map(Some)
        })
        .await
        .map_err(join_error)??;
        if let Some(update) = update {
            let replicas = self.permission_replicas.lock().await;
            let Some(replicas) = replicas.as_ref() else {
                return Ok(());
            };
            // A running Raft replica receives snapshots through its own protocol.
            if !replicas.contains_key(&peer.space_id) {
                log.receive_roster_update(update, host, self.device_id)
                    .await
                    .map_err(network_error)?;
            }
        }
        if let Some(membership) = log.membership().await.map_err(network_error_box)? {
            self.cache_roster(store, &peer.space_id, &membership)
                .await?;
        }
        Ok(())
    }

    async fn cache_roster(
        &self,
        store: &SyncPreview,
        space_id: &str,
        membership: &Membership,
    ) -> Result<(), AppError> {
        let peers: Vec<_> = membership
            .peer_contacts()
            .filter(|(device, _)| device.enrollment.device != self.device_id)
            .map(|(_, contact)| {
                let name = device_name(&contact.certificate);
                let address = contact
                    .address_hint
                    .map_or_else(|| format!("mdns:{name}"), |addr| addr.to_string());
                PairedDevice {
                    space_id: space_id.into(),
                    certificate: contact.certificate.clone(),
                    name,
                    address: Some(address),
                    display_name: None,
                }
            })
            .collect();
        store
            .with_store(move |store| store.cache_roster_peers(&peers))
            .await
    }

    async fn dial(
        &self,
        db: &DbPool,
        store: &SyncPreview,
        peer: &PairedDevice,
        address: String,
        enrollment: Option<Enrollment>,
    ) -> Result<(), AppError> {
        let addresses = resolve_peer(address).await?;
        let directory = self.directory.clone();
        let certificate = peer.certificate.clone();
        let identity = tokio::task::spawn_blocking(move || {
            Identity::with_certificates(&directory, vec![certificate.into()]).map_err(network_error)
        })
        .await
        .map_err(join_error)??;
        let connection = match enrollment {
            Some(peer) => PeerConnection::ConnectPaired(addresses, peer),
            None => PeerConnection::Connect(addresses),
        };
        store
            .exchange_peer(db, peer.space_id.clone(), identity, connection)
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

fn network_error_box(error: impl std::error::Error + Send + Sync + 'static) -> AppError {
    network_error(Box::new(error))
}
fn join_error(error: tokio::task::JoinError) -> AppError {
    AppError::Internal(format!("sync task: {error}"))
}
