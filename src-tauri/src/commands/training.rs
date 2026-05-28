use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

use crate::{
    classifier::{
        dataset::load_approved,
        trainer::{make_optimizer, precompute_embeddings, split_indices, TrainingConfig},
        TrainableClassifier,
    },
    db::DbPool,
    error::AppError,
    ClassifierState,
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

fn models_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?
        .join("models");
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
            if let Some(rest) = s.strip_prefix("model_v") {
                if let Some(n) = rest.strip_suffix(".safetensors") {
                    if let Ok(v) = n.parse::<u32>() {
                        max = max.max(v);
                    }
                }
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
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Minimal ISO-8601 UTC representation without pulling in chrono here.
    let s = secs;
    let mins = s / 60;
    let hours = mins / 60;
    let days_total = hours / 24;
    let sec = s % 60;
    let min = mins % 60;
    let hour = hours % 24;
    // Approximate calendar date from epoch (good enough for a card timestamp).
    let (year, month, day) = epoch_days_to_ymd(days_total);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn is_leap(year: u64) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn epoch_days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let days_per_month = [
        31u64,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for &d in &days_per_month {
        if days < d {
            break;
        }
        days -= d;
        month += 1;
    }
    (year, month, days + 1)
}

// ---------------------------------------------------------------------------
// Active model helpers (stored in app_settings)
// ---------------------------------------------------------------------------

async fn get_active_version(pool: &DbPool) -> u32 {
    sqlx::query_file!("queries/training/get_setting.sql", SETTING_ACTIVE_MODEL)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.value.parse().ok())
        .unwrap_or(0)
}

async fn set_active_version(pool: &DbPool, version: u32) -> Result<(), AppError> {
    let v = version.to_string();
    sqlx::query_file!(
        "queries/training/upsert_setting.sql",
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
    user_id: String,
    config: FinetuneConfig,
    app: AppHandle,
    db: State<'_, DbPool>,
    classifier: State<'_, ClassifierState>,
    history: State<'_, TrainingHistory>,
    cancel: State<'_, CancelToken>,
) -> Result<FinetuneResult, AppError> {
    let resource = resource_dir(&app)?;
    let resource2 = resource.clone();
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?;
    let app_data2 = app_data.clone();
    let dir = models_dir(&app)?;
    let version = next_version(&dir);

    // Resolve starting weights: active finetuned version, or base.
    let active = get_active_version(&db).await;
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
    let samples = load_approved(&db, &user_id, &tokenizer, &classes).await?;

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

    let save_path = weights_path(&dir, version);
    let classes_clone = classes.clone();
    let train_handle = tokio::task::spawn_blocking(move || -> Result<FinetuneResult, AppError> {
        let trainable = if from_scratch {
            TrainableClassifier::load_fresh(&resource, &app_data, classes_clone)?
        } else {
            TrainableClassifier::load(&resource, &app_data, &start_weights, classes_clone)?
        };

        let split = split_indices(samples.len(), 0.8);
        let mut optimizer = make_optimizer(&trainable.var_map, &training_config)?;
        let num_classes = trainable.classes.len();

        // Pre-compute all MiniLM embeddings once — frozen encoder never
        // runs again during the epoch loop.
        let embeddings = precompute_embeddings(
            &trainable.encoder,
            &samples,
            num_classes,
            training_config.batch_size,
            &trainable.device,
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
            result = &mut train_handle => {
                // Drain any remaining messages before breaking.
                while let Ok(progress) = rx.try_recv() {
                    if let Ok(mut h) = history.lock() {
                        h.push(progress.clone());
                    }
                    let _ = app.emit("training://progress", ());
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

        if let Ok(cl) = crate::classifier::Classifier::load(&resource2, &app_data2, classes) {
            if let Ok(mut guard) = classifier.lock() {
                *guard = Some(cl);
                let _ = app.emit("classifier://ready", ());
            }
        }
    }

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
) -> Result<Vec<ModelCard>, AppError> {
    let dir = models_dir(&app)?;
    let active = get_active_version(&db).await;

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
) -> Result<(), AppError> {
    let dir = models_dir(&app)?;

    let p = weights_path(&dir, version);
    if !p.exists() {
        return Err(AppError::NotFound(format!("model v{version} not found")));
    }
    let weights = p;

    set_active_version(&db, version).await?;
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
    user_id: String,
    limit: i64,
    db: State<'_, DbPool>,
    classifier: State<'_, ClassifierState>,
) -> Result<Vec<TrainingSample>, AppError> {
    let rows = sqlx::query_file!("queries/training/get_training_samples.sql", user_id, limit)
        .fetch_all(&*db)
        .await
        .map_err(|e| AppError::Db(format!("get_training_samples: {e}")))?;

    let guard = classifier
        .lock()
        .map_err(|e| AppError::Internal(format!("lock: {e}")))?;
    let cl = guard
        .as_ref()
        .ok_or_else(|| AppError::Internal("classifier not ready".to_string()))?;

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
    user_id: String,
    db: State<'_, DbPool>,
) -> Result<i64, AppError> {
    let row = sqlx::query_file!("queries/training/count_approved_transactions.sql", user_id)
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
) -> Result<(), AppError> {
    if version == 0 {
        return Err(AppError::InvalidInput(
            "cannot delete the base model".to_string(),
        ));
    }

    let dir = models_dir(&app)?;
    let active = get_active_version(&db).await;

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
        sqlx::query_file!("queries/settings/delete_setting.sql", SETTING_ACTIVE_MODEL)
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
