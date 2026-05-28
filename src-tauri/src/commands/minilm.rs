use crate::AppError;
use hf_hub::api::tokio::ApiBuilder;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

const MODEL_ID: &str = "sentence-transformers/all-MiniLM-L6-v2";

// Files we need from the HF repo.
const FILES: &[&str] = &["config.json", "tokenizer.json", "model.safetensors"];

fn minilm_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("app_data_dir: {e}")))?
        .join("minilm");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(format!("create minilm dir: {e}")))?;
    Ok(dir)
}

#[derive(Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub file: String,
    pub step: u8,
    pub total: u8,
}

/// Returns true if all three MiniLM files are present in the local cache.
#[tauri::command]
pub async fn minilm_status(app: AppHandle) -> Result<bool, AppError> {
    let dir = minilm_dir(&app)?;
    Ok(FILES.iter().all(|f| dir.join(f).exists()))
}

/// Downloads the three MiniLM files from HuggingFace Hub into
/// `app_data_dir()/minilm/`. Emits `minilm://progress` events as each file
/// completes. No-ops for files that are already present.
#[tauri::command]
pub async fn ensure_minilm(app: AppHandle) -> Result<(), AppError> {
    let dir = minilm_dir(&app)?;
    let total = FILES.len() as u8;

    let api = ApiBuilder::new()
        .build()
        .map_err(|e| AppError::Internal(format!("hf-hub build: {e}")))?;
    let repo = api.model(MODEL_ID.to_string());

    for (i, filename) in FILES.iter().enumerate() {
        let dest = dir.join(filename);
        if dest.exists() {
            let _ = app.emit(
                "minilm://progress",
                DownloadProgress {
                    file: filename.to_string(),
                    step: i as u8 + 1,
                    total,
                },
            );
            continue;
        }

        // `get` checks the hf-hub cache first, then downloads.
        let cached = repo
            .get(filename)
            .await
            .map_err(|e| AppError::Internal(format!("download {filename}: {e}")))?;

        // Copy from the hf-hub cache into our own minilm dir so the path is
        // stable and independent of the hf-hub cache layout.
        std::fs::copy(&cached, &dest).map_err(|e| AppError::Io(format!("copy {filename}: {e}")))?;

        let _ = app.emit(
            "minilm://progress",
            DownloadProgress {
                file: filename.to_string(),
                step: i as u8 + 1,
                total,
            },
        );
    }

    Ok(())
}
