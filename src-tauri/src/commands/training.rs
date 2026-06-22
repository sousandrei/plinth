use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

use crate::{
    ClassifierState, Session,
    classifier::{
        Classifier, TrainableClassifier,
        dataset::load_approved,
        trainer::{TrainingConfig, make_optimizer, precompute_embeddings, split_indices},
    },
    db::DbPool,
    error::AppError,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SETTING_ACTIVE_MODEL: &str = "active_model_version";
const MIN_SAMPLES: usize = 20;

// ---------------------------------------------------------------------------
// Shared training-run state (epoch history for the current/last run)
// ---------------------------------------------------------------------------

/// Managed state: the progress history for the current (or most recent) run.
/// Cleared at the start of each `fine_tune` call and appended every epoch.
pub type TrainingHistory = Mutex<Vec<FinetuneProgress>>;

/// Managed state: cancellation flag for the current training run.
/// Set to `true` by `stop_training`; checked after each epoch.
#[derive(Default)]
pub struct CancelToken(pub Arc<AtomicBool>);

// ---------------------------------------------------------------------------
// Public types (cross IPC boundary)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FinetuneConfig {
    pub epochs: u32,
    pub batch_size: usize,
    pub learning_rate: f64,
    #[serde(default)]
    pub from_scratch: bool,
}

impl Default for FinetuneConfig {
    fn default() -> Self {
        let d = TrainingConfig::default();
        Self {
            epochs: d.epochs,
            batch_size: d.batch_size,
            learning_rate: d.learning_rate,
            from_scratch: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FinetuneProgress {
    pub epoch: u32,
    pub total_epochs: u32,
    pub train_loss: f32,
    pub train_accuracy: f32,
    pub val_loss: f32,
    pub val_accuracy: f32,
}

/// Emitted during the MiniLM encoder precompute pass that runs before the
/// epoch loop. `total` is the number of approved samples; `current` grows
/// in `batch_size` increments as each encoder batch completes.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmbedProgress {
    pub current: u32,
    pub total: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinetuneResult {
    pub version: u32,
    pub epochs_completed: u32,
    pub final_train_loss: f32,
    pub final_train_accuracy: f32,
    pub final_val_loss: f32,
    pub final_val_accuracy: f32,
    pub samples_used: usize,
    pub epoch_history: Vec<FinetuneProgress>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelCard {
    /// 0 means the shipped base model.
    pub version: u32,
    pub trained_at: String,
    pub epochs: u32,
    pub samples_used: usize,
    pub train_loss: Option<f32>,
    pub train_accuracy: f32,
    pub val_loss: Option<f32>,
    pub val_accuracy: f32,
    pub is_base: bool,
    pub is_active: bool,
    #[serde(default)]
    pub epoch_history: Vec<FinetuneProgress>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TrainingSample {
    pub id: String,
    pub text: String,
    pub amount: i64,
    pub booking_date: String,
    pub actual_category: String,
    pub predicted_category: String,
}

// ---------------------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------------------

fn models_dir(app: &AppHandle, space_id: &str) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?
        .join("models")
        .join(space_id);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(format!("create models dir: {e}")))?;
    Ok(dir)
}

fn weights_path(dir: &Path, version: u32) -> PathBuf {
    dir.join(format!("model_v{version}.safetensors"))
}

fn card_path(dir: &Path, version: u32) -> PathBuf {
    dir.join(format!("model_v{version}.json"))
}

fn resource_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .resource_dir()
        .map_err(|e| AppError::Internal(format!("resource_dir: {e}")))
}

fn next_version(dir: &PathBuf) -> u32 {
    // Scan for existing model_vN.safetensors and return N+1.
    let mut max = 0u32;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if let Some(rest) = s.strip_prefix("model_v")
                && let Some(n) = rest.strip_suffix(".safetensors")
                && let Ok(v) = n.parse::<u32>()
            {
                max = max.max(v);
            }
        }
    }
    max + 1
}

fn read_card(dir: &Path, version: u32) -> Option<ModelCard> {
    let data = std::fs::read_to_string(card_path(dir, version)).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_card(dir: &Path, card: &ModelCard) -> Result<(), AppError> {
    let json = serde_json::to_string_pretty(card)
        .map_err(|e| AppError::Internal(format!("serialise card: {e}")))?;
    std::fs::write(card_path(dir, card.version), json)
        .map_err(|e| AppError::Io(format!("write card: {e}")))
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

// ---------------------------------------------------------------------------
// Active model helpers (stored in app_settings)
// ---------------------------------------------------------------------------

async fn get_active_version(pool: &DbPool, space_id: &str) -> u32 {
    sqlx::query_file!(
        "queries/training/get_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(|r| r.value.parse().ok())
    .unwrap_or(0)
}

async fn set_active_version(pool: &DbPool, space_id: &str, version: u32) -> Result<(), AppError> {
    let v = version.to_string();
    sqlx::query_file!(
        "queries/training/upsert_setting.sql",
        space_id,
        SETTING_ACTIVE_MODEL,
        v
    )
    .execute(pool)
    .await
    .map_err(|e| AppError::Db(format!("set_active_version: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fine_tune(
    config: FinetuneConfig,
    app: AppHandle,
    db: State<'_, DbPool>,
    classifier: State<'_, ClassifierState>,
    history: State<'_, TrainingHistory>,
    cancel: State<'_, CancelToken>,
    session: State<'_, Session>,
) -> Result<FinetuneResult, AppError> {
    let active_session = session.require()?;
    let space_id = active_session.space_id.clone();

    let resource = resource_dir(&app)?;
    let resource2 = resource.clone();
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?;
    let app_data2 = app_data.clone();
    let dir = models_dir(&app, &space_id)?;
    let version = next_version(&dir);

    // Resolve starting weights: active finetuned version, or base.
    let active = get_active_version(&db, &space_id).await;
    let start_weights = if active > 0 {
        weights_path(&dir, active)
    } else {
        resource.join("model.safetensors")
    };

    // Extract tokenizer and classes. If the classifier is already loaded use
    // it directly; otherwise load them from the MiniLM cache and resource dir.
    // This allows from-scratch training even before any head weights exist.
    let existing_tokenizer_classes = {
        let guard = classifier
            .lock()
            .map_err(|e| AppError::Internal(format!("lock: {e}")))?;
        guard
            .as_ref()
            .map(|cl| (cl.tokenizer().clone(), cl.classes().to_vec()))
    };

    let (tokenizer, classes) = match existing_tokenizer_classes {
        Some(val) => val,
        None => {
            let minilm_dir = app_data.join("minilm");
            let tokenizer = tokenizers::Tokenizer::from_file(minilm_dir.join("tokenizer.json"))
                .map_err(|e| AppError::Internal(format!("load tokenizer: {e}")))?;
            let classes = sqlx::query_file!("queries/categories/list_all_categories.sql")
                .fetch_all(db.inner())
                .await
                .map_err(|e| AppError::Db(format!("load classes: {e}")))?
                .into_iter()
                .map(|r| r.name)
                .collect::<Vec<String>>();
            (tokenizer, classes)
        }
    };
    let samples = load_approved(&db, &space_id, &tokenizer, &classes).await?;

    if samples.len() < MIN_SAMPLES {
        return Err(AppError::InvalidInput(format!(
            "need at least {MIN_SAMPLES} approved transactions to fine-tune, found {}",
            samples.len()
        )));
    }

    let samples_used = samples.len();
    let total_epochs = config.epochs;

    history
        .lock()
        .map_err(|e| AppError::Internal(format!("history lock: {e}")))?
        .clear();

    cancel.0.store(false, Ordering::Relaxed);
    let cancel_flag = Arc::clone(&cancel.0);

    let from_scratch = config.from_scratch;
    let training_config = TrainingConfig {
        epochs: config.epochs,
        batch_size: config.batch_size,
        learning_rate: config.learning_rate,
        weight_decay: 1e-4,
    };

    let (tx, mut rx) = mpsc::unbounded_channel::<FinetuneProgress>();
    let (embed_tx, mut embed_rx) = mpsc::unbounded_channel::<EmbedProgress>();

    let save_path = weights_path(&dir, version);
    let classes_clone = classes.clone();
    let total_samples_u32 = u32::try_from(samples.len()).unwrap_or(u32::MAX);
    let train_handle = tokio::task::spawn_blocking(move || -> Result<FinetuneResult, AppError> {
        let trainable = if from_scratch {
            TrainableClassifier::load_fresh(&app_data, classes_clone)?
        } else {
            TrainableClassifier::load(&resource, &app_data, &start_weights, classes_clone)?
        };

        let split = split_indices(samples.len(), 0.8);
        let mut optimizer = make_optimizer(&trainable.var_map, &training_config)?;
        let num_classes = trainable.classes.len();

        // Pre-compute all MiniLM embeddings once — frozen encoder never
        // runs again during the epoch loop. Emit a progress event per
        // batch so the UI can show long precompute passes on big datasets.
        let _ = embed_tx.send(EmbedProgress {
            current: 0,
            total: total_samples_u32,
        });
        let embeddings = precompute_embeddings(
            &trainable.encoder,
            &samples,
            training_config.batch_size,
            &trainable.device,
            |embedded, _total| {
                let _ = embed_tx.send(EmbedProgress {
                    current: u32::try_from(embedded).unwrap_or(u32::MAX),
                    total: total_samples_u32,
                });
            },
        )?;

        let mut last_result = None;
        let mut epoch_history: Vec<FinetuneProgress> = Vec::new();

        for epoch in 1..=training_config.epochs {
            let epoch_result = crate::classifier::trainer::run_epoch(
                &embeddings,
                &trainable.head,
                &mut optimizer,
                &trainable.var_map,
                &samples,
                &split,
                &training_config,
                epoch,
                num_classes,
                &trainable.device,
            )?;

            let progress = FinetuneProgress {
                epoch,
                total_epochs,
                train_loss: epoch_result.train_loss,
                train_accuracy: epoch_result.train_accuracy,
                val_loss: epoch_result.val_loss,
                val_accuracy: epoch_result.val_accuracy,
            };

            epoch_history.push(progress.clone());
            let _ = tx.send(progress);
            last_result = Some(epoch_result);

            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }
        }

        trainable.save(&save_path)?;

        let last = last_result.unwrap();
        let epochs_completed = last.epoch;
        Ok(FinetuneResult {
            version,
            epochs_completed,
            final_train_loss: last.train_loss,
            final_train_accuracy: last.train_accuracy,
            final_val_loss: last.val_loss,
            final_val_accuracy: last.val_accuracy,
            samples_used,
            epoch_history,
        })
    });

    // Drain progress events as they arrive: append to shared state and emit.
    tokio::pin!(train_handle);
    let result = loop {
        tokio::select! {
            Some(progress) = rx.recv() => {
                if let Ok(mut h) = history.lock() {
                    h.push(progress.clone());
                }
                let _ = app.emit("training://progress", ());
            }
            Some(embed) = embed_rx.recv() => {
                let _ = app.emit("training://precompute", &embed);
            }
            result = &mut train_handle => {
                // Drain any remaining messages before breaking.
                while let Ok(progress) = rx.try_recv() {
                    if let Ok(mut h) = history.lock() {
                        h.push(progress.clone());
                    }
                    let _ = app.emit("training://progress", ());
                }
                while let Ok(embed) = embed_rx.try_recv() {
                    let _ = app.emit("training://precompute", &embed);
                }
                let result = result.map_err(|e| AppError::Internal(format!("training task: {e}")))??;
                break result;
            }
        }
    };

    // Persist the model card.
    let card = ModelCard {
        version,
        trained_at: now_iso(),
        epochs: result.epochs_completed,
        samples_used: result.samples_used,
        train_loss: Some(result.final_train_loss),
        train_accuracy: result.final_train_accuracy,
        val_loss: Some(result.final_val_loss),
        val_accuracy: result.final_val_accuracy,
        is_base: false,
        is_active: false,
        epoch_history: result.epoch_history.clone(),
    };
    write_card(&dir, &card)?;

    // If the classifier wasn't loaded before (first from-scratch run), load it
    // now using the weights we just saved so the app becomes ready immediately.
    let needs_loading = {
        let guard = classifier
            .lock()
            .map_err(|e| AppError::Internal(format!("lock post-train check: {e}")))?;
        guard.is_none()
    };

    if needs_loading {
        let saved_weights = weights_path(&dir, version);
        // Copy the new weights to the resource dir as the new base so
        // subsequent loads and reverts work correctly.
        let base_dest = resource2.join("model.safetensors");
        let _ = std::fs::copy(&saved_weights, &base_dest);

        let classes = sqlx::query_file!("queries/categories/list_all_categories.sql")
            .fetch_all(db.inner())
            .await
            .map_err(|e| AppError::Db(format!("post-train load classes: {e}")))?
            .into_iter()
            .map(|r| r.name)
            .collect::<Vec<String>>();

        if let Ok(cl) = crate::classifier::Classifier::load(&resource2, &app_data2, classes)
            && let Ok(mut guard) = classifier.lock()
        {
            *guard = Some(cl);
            let _ = app.emit("classifier://ready", ());
        }
    }

    set_active_version(&db, &space_id, version).await?;

    // Notify the UI that the model list has changed.
    let _ = app.emit("training://done", ());

    Ok(result)
}

#[tauri::command]
pub async fn get_training_progress(
    history: State<'_, TrainingHistory>,
) -> Result<Vec<FinetuneProgress>, AppError> {
    let h = history
        .lock()
        .map_err(|e| AppError::Internal(format!("history lock: {e}")))?;
    Ok(h.clone())
}

#[tauri::command]
pub async fn list_models(
    app: AppHandle,
    db: State<'_, DbPool>,
    session: State<'_, Session>,
) -> Result<Vec<ModelCard>, AppError> {
    let active_session = session.require()?;
    let dir = models_dir(&app, &active_session.space_id)?;
    let active = get_active_version(&db, &active_session.space_id).await;

    let mut cards: Vec<ModelCard> = Vec::new();

    // Versioned models — read from card JSON files on disk.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut versions: Vec<u32> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy().into_owned();
                let rest = s.strip_prefix("model_v")?.strip_suffix(".json")?;
                rest.parse::<u32>().ok()
            })
            .collect();
        versions.sort_unstable();

        for v in versions {
            if let Some(mut card) = read_card(&dir, v) {
                card.is_active = active == v;
                cards.push(card);
            }
        }
    }

    Ok(cards)
}

#[tauri::command]
pub async fn set_active_model(
    version: u32,
    app: AppHandle,
    db: State<'_, DbPool>,
    classifier: State<'_, ClassifierState>,
    session: State<'_, Session>,
) -> Result<(), AppError> {
    let active_session = session.require()?;
    let dir = models_dir(&app, &active_session.space_id)?;

    let p = weights_path(&dir, version);
    if !p.exists() {
        return Err(AppError::NotFound(format!("model v{version} not found")));
    }
    let weights = p;

    set_active_version(&db, &active_session.space_id, version).await?;
    let mut guard = classifier
        .lock()
        .map_err(|e| AppError::Internal(format!("lock: {e}")))?;
    if let Some(cl) = guard.as_mut() {
        cl.load_version(&weights)?;
    }

    Ok(())
}

#[tauri::command]
pub async fn get_training_samples(
    limit: i64,
    version: Option<u32>,
    app: AppHandle,
    db: State<'_, DbPool>,
    classifier: State<'_, ClassifierState>,
    session: State<'_, Session>,
) -> Result<Vec<TrainingSample>, AppError> {
    let active_session = session.require()?;
    let space_id = active_session.space_id.clone();

    let rows = sqlx::query_file!("queries/training/get_training_samples.sql", space_id, limit)
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("get_training_samples: {e}")))?;

    // Resolve the active version before acquiring the classifier lock (no await inside lock).
    let active_version = get_active_version(&db, &space_id).await;
    let requested = version.unwrap_or(active_version);
    let swapped = requested != active_version;

    // Pre-resolve paths before the lock so we don't hold it across any awaits.
    let requested_weights = if swapped {
        let dir = models_dir(&app, &space_id)?;
        let weights = weights_path(&dir, requested);
        if !weights.exists() {
            return Err(AppError::NotFound(format!(
                "model v{requested} weights not found"
            )));
        }
        Some(weights)
    } else {
        None
    };
    let active_weights_path = if swapped {
        let dir = models_dir(&app, &space_id)?;
        Some(weights_path(&dir, active_version))
    } else {
        None
    };

    let mut guard = classifier
        .lock()
        .map_err(|e| AppError::Internal(format!("lock: {e}")))?;
    let cl = guard
        .as_mut()
        .ok_or_else(|| AppError::Internal("classifier not ready".to_string()))?;

    if let Some(ref weights) = requested_weights {
        cl.load_version(weights)
            .map_err(|e| AppError::Internal(format!("load_version v{requested}: {e}")))?;
    }

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let predicted = cl
            .predict(&row.text, row.amount, &row.booking_date)
            .unwrap_or_default();
        out.push(TrainingSample {
            id: row.id,
            text: row.text,
            amount: row.amount,
            booking_date: row.booking_date,
            actual_category: row.category,
            predicted_category: predicted,
        });
    }

    // Restore the active model's weights if we swapped them out.
    if let Some(ref active_w) = active_weights_path
        && active_w.exists()
    {
        cl.load_version(active_w)
            .map_err(|e| AppError::Internal(format!("restore active v{active_version}: {e}")))?;
    }

    Ok(out)
}

#[tauri::command]
pub async fn is_classifier_ready(classifier: State<'_, ClassifierState>) -> Result<bool, AppError> {
    let guard = classifier
        .lock()
        .map_err(|e| AppError::Internal(format!("lock: {e}")))?;
    Ok(guard.is_some())
}

#[tauri::command]
pub async fn count_approved_transactions(
    db: State<'_, DbPool>,
    session: State<'_, Session>,
) -> Result<i64, AppError> {
    let active_session = session.require()?;
    let row = sqlx::query_file!(
        "queries/training/count_approved_transactions.sql",
        active_session.space_id
    )
    .fetch_one(&*db)
    .await
    .map_err(|e| AppError::Db(format!("count_approved_transactions: {e}")))?;
    Ok(row.count)
}

#[tauri::command]
pub async fn stop_training(cancel: State<'_, CancelToken>) -> Result<(), AppError> {
    cancel.0.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn delete_model(
    version: u32,
    app: AppHandle,
    db: State<'_, DbPool>,
    classifier: State<'_, ClassifierState>,
    session: State<'_, Session>,
) -> Result<(), AppError> {
    if version == 0 {
        return Err(AppError::InvalidInput(
            "cannot delete the base model".to_string(),
        ));
    }

    let active_session = session.require()?;
    let space_id = active_session.space_id.clone();
    let dir = models_dir(&app, &space_id)?;
    let active = get_active_version(&db, &space_id).await;

    let w = weights_path(&dir, version);
    let c = card_path(&dir, version);

    if w.exists() {
        std::fs::remove_file(&w).map_err(|e| AppError::Io(format!("remove weights: {e}")))?;
    }
    if c.exists() {
        std::fs::remove_file(&c).map_err(|e| AppError::Io(format!("remove card: {e}")))?;
    }

    // If the deleted version was active, unload the classifier — no base model exists.
    // The user must train a new version to restore predictions.
    if active == version {
        sqlx::query_file!(
            "queries/training/delete_setting.sql",
            space_id,
            SETTING_ACTIVE_MODEL
        )
        .execute(&*db)
        .await
        .map_err(|e| AppError::Db(format!("delete_active_model_setting: {e}")))?;
        let mut guard = classifier
            .lock()
            .map_err(|e| AppError::Internal(format!("lock: {e}")))?;
        *guard = None;
    } else {
        // A non-active version was deleted; reload the still-active version.
        let mut guard = classifier
            .lock()
            .map_err(|e| AppError::Internal(format!("lock: {e}")))?;
        if let Some(cl) = guard.as_mut() {
            let _ = cl.load_version(&weights_path(&dir, active));
        }
    }

    Ok(())
}

#[tauri::command]
pub fn get_training_device() -> String {
    #[cfg(feature = "cuda")]
    {
        "CUDA".to_string()
    }
    #[cfg(all(target_os = "macos", not(feature = "cuda")))]
    {
        "Metal".to_string()
    }
    #[cfg(all(not(target_os = "macos"), not(feature = "cuda")))]
    {
        "CPU".to_string()
    }
}

/// Reload the classifier from disk. Used after MiniLM download so the
/// classifier (which failed to load at startup when the files were missing)
/// can be loaded now that the weights are available.
#[tauri::command]
pub async fn reload_classifier(
    app: AppHandle,
    db: State<'_, DbPool>,
    classifier: State<'_, ClassifierState>,
) -> Result<(), AppError> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| AppError::Internal(format!("resource_dir: {e}")))?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?;

    let classes = sqlx::query_file!("queries/categories/list_all_categories.sql")
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("fetch categories: {e}")))?
        .into_iter()
        .map(|r| r.name)
        .collect::<Vec<String>>();

    let cl = Classifier::load(&resource_dir, &app_data_dir, classes)
        .map_err(|e| AppError::Internal(format!("reload classifier: {e}")))?;

    if let Ok(mut guard) = classifier.lock() {
        *guard = Some(cl);
    }

    Ok(())
}
