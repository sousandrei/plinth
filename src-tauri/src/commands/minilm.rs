use crate::AppError;
use hf_hub::{HFClient, split_id};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

const MODEL_ID: &str = "sentence-transformers/all-MiniLM-L6-v2";

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

    let client = HFClient::new().map_err(|e| AppError::Internal(format!("hf-hub client: {e}")))?;
    let (owner, name) = split_id(MODEL_ID);
    let repo = client.model(owner, name);

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

        let cached = repo
            .download_file()
            .filename(filename.to_string())
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("download {filename}: {e}")))?;

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

    let _ = app.emit("minilm://ready", ());

    Ok(())
}
