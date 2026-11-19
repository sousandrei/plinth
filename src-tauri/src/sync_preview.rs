use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use plinth_sync::{Fields, Session as PeerSession, Store, transport};
use sqlx::SqliteConnection;
use tauri::State;
use tokio::net::TcpListener;

use crate::{
    Session,
    commands::transactions::{apply_pending_transaction_edit, valid_sync_fields},
    db::DbPool,
    error::AppError,
};

pub const IDENTIFIER: &str = "plinth-sync-preview";

#[derive(Clone, Default)]
pub struct SyncPreview(Option<Arc<PreviewStore>>);

struct PreviewStore {
    _profile_lock: File,
    directory: PathBuf,
    store: Mutex<Option<Store>>,
}

#[tauri::command]
pub async fn sync_preview_exchange(
    address: String,
    listen: bool,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    preview: State<'_, SyncPreview>,
) -> Result<(), AppError> {
    let paired = preview.0.as_ref().ok_or(AppError::Forbidden)?;
    let space_id = session.require()?.space_id;
    let address: SocketAddr = address
        .parse()
        .map_err(|error| AppError::InvalidInput(format!("peer address: {error}")))?;
    if address.ip().is_unspecified() || address.port() == 0 {
        return Err(AppError::InvalidInput(
            "use a specific IP address and port".into(),
        ));
    }
    let directory = paired.directory.join("peer");
    let identity = tokio::task::spawn_blocking(move || transport::Identity::load(&directory))
        .await
        .map_err(task_error)?
        .map_err(peer_error)?;
    let socket = if listen {
        let listener = TcpListener::bind(address).await.map_err(peer_io_error)?;
        Some(accept_peer(&listener).await?)
    } else {
        None
    };
    preview
        .exchange_peer(db.inner(), space_id, identity, address, socket)
        .await
}

pub(crate) async fn accept_peer(listener: &TcpListener) -> Result<TcpStream, AppError> {
    let (socket, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
        .await
        .map_err(|error| AppError::Io(format!("waiting for paired peer: {error}")))?
        .map_err(peer_io_error)?;
    let socket = socket.into_std().map_err(peer_io_error)?;
    socket.set_nonblocking(false).map_err(peer_io_error)?;
    Ok(socket)
}

impl SyncPreview {
    pub(crate) async fn open(directory: PathBuf) -> Result<Self, AppError> {
        tokio::task::spawn_blocking(move || {
            let profile_lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(directory.join("sync-preview.lock"))
                .map_err(|error| AppError::Io(format!("open sync preview lock: {error}")))?;
            profile_lock
                .try_lock()
                .map_err(|error| AppError::Io(format!("lock sync preview profile: {error}")))?;
            let store = Store::open(directory.join("sync.sqlite")).map_err(store_error)?;
            Ok(Self(Some(Arc::new(PreviewStore {
                _profile_lock: profile_lock,
                directory,
                store: Mutex::new(Some(store)),
            }))))
        })
        .await
        .map_err(task_error)?
    }

    pub(crate) fn active(&self) -> bool {
        self.0.is_some()
    }

    #[cfg(test)]
    pub(crate) async fn stage(
        &self,
        space_id: String,
        record_id: String,
        fields: Fields,
    ) -> Result<(), AppError> {
        self.stage_batch(space_id, vec![(record_id, fields)]).await
    }

    pub(crate) async fn stage_batch(
        &self,
        space_id: String,
        edits: Vec<(String, Fields)>,
    ) -> Result<(), AppError> {
        let timestamp_ms = chrono::Utc::now().timestamp_millis();
        self.with_store(move |store| {
            store.stage_edits(
                &space_id,
                timestamp_ms,
                edits.iter().map(|(id, fields)| (id.as_str(), fields)),
            )
        })
        .await
    }

    pub(crate) async fn replay(&self, conn: &mut SqliteConnection) -> Result<(), AppError> {
        while let Some(edit) = self.with_store(|store| store.next_pending_edit()).await? {
            apply_pending_transaction_edit(&mut *conn, &edit).await?;
            self.with_store(move |store| store.complete_edit(edit.sequence))
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn exchange_peer(
        &self,
        db: &DbPool,
        space_id: String,
        identity: transport::Identity,
        address: SocketAddr,
        accepted: Option<TcpStream>,
    ) -> Result<(), AppError> {
        let preview = self.clone();
        let db = db.clone();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let (mut stream, initiator): (Box<dyn ReadWrite>, _) = match accepted {
                Some(socket) => (
                    Box::new(identity.accept(socket).map_err(peer_error)?),
                    false,
                ),
                None => (
                    Box::new(identity.connect(address).map_err(peer_error)?),
                    true,
                ),
            };
            let mut session = PeerSession::new(&space_id);
            transport::exchange(&mut stream, &space_id, initiator, |incoming| {
                let (returned_session, outgoing) = runtime.block_on(preview.exchange_step(
                    &db,
                    std::mem::replace(&mut session, PeerSession::new(&space_id)),
                    incoming.map(<[u8]>::to_vec),
                ))?;
                session = returned_session;
                Ok(outgoing)
            })
            .map_err(peer_error)
        })
        .await
        .map_err(task_error)?
    }

    async fn exchange_step(
        &self,
        db: &DbPool,
        mut session: PeerSession,
        incoming: Option<Vec<u8>>,
    ) -> Result<(PeerSession, Option<Vec<u8>>), AppError> {
        let mut conn = db.acquire().await?;
        let result = self
            .with_store(move |store| {
                if let Some(bytes) = incoming {
                    store.receive_projected(&mut session, &bytes, valid_sync_fields)?;
                }
                let outgoing = store.next_message(&mut session)?;
                Ok((session, outgoing))
            })
            .await?;
        self.replay(&mut conn).await?;
        Ok(result)
    }

    pub(crate) async fn reset(&self) -> Result<(), AppError> {
        let Some(preview) = self.0.clone() else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || {
            let mut state = preview.store.lock().map_err(|_| poisoned_store())?;
            drop(state.take());
            remove_sync_files(&preview.directory)
        })
        .await
        .map_err(task_error)?
    }

    async fn with_store<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Store) -> Result<T, plinth_sync::Error> + Send + 'static,
    ) -> Result<T, AppError> {
        let preview = self
            .0
            .clone()
            .ok_or_else(|| AppError::Internal("sync preview is disabled".into()))?;
        tokio::task::spawn_blocking(move || {
            let mut state = preview.store.lock().map_err(|_| poisoned_store())?;
            let store = state
                .as_mut()
                .ok_or_else(|| AppError::Internal("sync preview store is closed".into()))?;
            operation(store).map_err(store_error)
        })
        .await
        .map_err(task_error)?
    }
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

fn peer_io_error(error: std::io::Error) -> AppError {
    AppError::Io(format!("sync peer: {error}"))
}

fn peer_error(error: Box<dyn std::error::Error + Send + Sync>) -> AppError {
    AppError::Io(format!("sync peer: {error}"))
}

fn remove_sync_files(directory: &Path) -> Result<(), AppError> {
    for name in ["sync.sqlite", "sync.sqlite-wal", "sync.sqlite-shm"] {
        match fs::remove_file(directory.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::Io(format!("remove {name}: {error}"))),
        }
    }
    Ok(())
}

fn store_error(error: plinth_sync::Error) -> AppError {
    AppError::Db(format!("sync preview: {error}"))
}

fn task_error(error: tokio::task::JoinError) -> AppError {
    AppError::Internal(format!("sync preview task: {error}"))
}

fn poisoned_store() -> AppError {
    AppError::Internal("sync preview store mutex poisoned".into())
}
