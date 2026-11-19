use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use plinth_sync::{Fields, Store};
use sqlx::SqliteConnection;

use crate::{commands::transactions::apply_pending_transaction_edit, error::AppError};

pub const IDENTIFIER: &str = "plinth-sync-preview";

#[derive(Clone, Default)]
pub struct SyncPreview(Option<Arc<PreviewStore>>);

struct PreviewStore {
    _profile_lock: File,
    directory: PathBuf,
    store: Mutex<Option<Store>>,
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

    pub(crate) async fn stage(
        &self,
        space_id: String,
        record_id: String,
        fields: Fields,
    ) -> Result<(), AppError> {
        let timestamp_ms = chrono::Utc::now().timestamp_millis();
        self.with_store(move |store| store.stage_edit(&space_id, &record_id, timestamp_ms, &fields))
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
