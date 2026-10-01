use std::path::{Path, PathBuf};

use burn::tensor::backend::Backend;
use tokenizers::Tokenizer;

use crate::error::AppError;

use super::encoder::{MiniLmConfig, MiniLmEncoder};

pub(super) fn minilm_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("minilm")
}

pub(super) fn load_tokenizer(minilm_dir: &Path) -> Result<Tokenizer, AppError> {
    Tokenizer::from_file(minilm_dir.join("tokenizer.json"))
        .map_err(|e| AppError::Internal(format!("load tokenizer: {e}")))
}

pub(super) fn load_encoder<B: Backend<FloatElem = f32, IntElem = i32>>(
    app_data_dir: &Path,
    device: &B::Device,
) -> Result<MiniLmEncoder<B>, AppError> {
    let dir = minilm_dir(app_data_dir);
    let weights_path = dir.join("model.safetensors");
    if !weights_path.exists() {
        return Err(AppError::NotFound(
            "MiniLM weights not cached — run ensure_minilm first".into(),
        ));
    }

    let data = std::fs::read_to_string(dir.join("config.json"))
        .map_err(|e| AppError::NotFound(format!("minilm config.json: {e}")))?;
    let config: MiniLmConfig = serde_json::from_str(&data)
        .map_err(|e| AppError::Internal(format!("parse MiniLM config: {e}")))?;
    MiniLmEncoder::load(&weights_path, &config, device)
}
