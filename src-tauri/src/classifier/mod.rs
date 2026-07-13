pub mod dataset;
pub mod features;
pub mod model;
pub mod trainer;

use candle_core::{DType, Device, Tensor};
use candle_nn::{VarBuilder, VarMap};
use candle_transformers::models::bert::Config as BertConfig;
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

use crate::error::AppError;
use features::build_features;
use model::{ClassificationHead, MiniLmEncoder};

pub fn best_device() -> Device {
    #[cfg(target_os = "macos")]
    {
        if let Ok(d) = Device::new_metal(0) {
            return d;
        }
    }
    #[cfg(feature = "cuda")]
    {
        if let Ok(d) = Device::new_cuda(0) {
            return d;
        }
    }
    Device::Cpu
}

fn minilm_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("minilm")
}

fn load_bert_config(minilm_dir: &Path) -> Result<BertConfig, AppError> {
    let data = std::fs::read_to_string(minilm_dir.join("config.json"))
        .map_err(|e| AppError::NotFound(format!("minilm config.json: {e}")))?;
    serde_json::from_str(&data).map_err(|e| AppError::Internal(format!("parse bert config: {e}")))
}

fn load_tokenizer(minilm_dir: &Path) -> Result<Tokenizer, AppError> {
    Tokenizer::from_file(minilm_dir.join("tokenizer.json"))
        .map_err(|e| AppError::Internal(format!("load tokenizer: {e}")))
}

fn load_encoder(app_data_dir: &Path, device: &Device) -> Result<MiniLmEncoder, AppError> {
    let dir = minilm_dir(app_data_dir);
    if !dir.join("model.safetensors").exists() {
        return Err(AppError::NotFound(
            "MiniLM weights not cached — run ensure_minilm first".into(),
        ));
    }
    let config = load_bert_config(&dir)?;
    MiniLmEncoder::load(&dir.join("model.safetensors"), &config, device)
        .map_err(|e| AppError::Internal(format!("load encoder: {e}")))
}

fn load_head(
    weights_path: &Path,
    num_classes: usize,
    device: &Device,
) -> Result<ClassificationHead, AppError> {
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, device)
            .map_err(|e| AppError::Internal(format!("mmap head weights: {e}")))?
    };
    ClassificationHead::new(num_classes, vb)
        .map_err(|e| AppError::Internal(format!("build head: {e}")))
}

// ---------------------------------------------------------------------------
// Classifier — inference only
// ---------------------------------------------------------------------------

pub struct Classifier {
    encoder: MiniLmEncoder,
    head: ClassificationHead,
    tokenizer: Tokenizer,
    classes: Vec<String>,
    device: Device,
    num_classes: usize,
}

impl Classifier {
    pub fn load(
        app_data_dir: &Path,
        head_weights: &Path,
        classes: Vec<String>,
    ) -> Result<Self, AppError> {
        let device = best_device();
        let encoder = load_encoder(app_data_dir, &device)?;
        let tokenizer = load_tokenizer(&minilm_dir(app_data_dir))?;
        let num_classes = classes.len();
        let head = load_head(head_weights, num_classes, &device)?;

        Ok(Self {
            encoder,
            head,
            tokenizer,
            classes,
            device,
            num_classes,
        })
    }

    pub fn load_version(&mut self, weights_path: &Path) -> Result<(), AppError> {
        self.head = load_head(weights_path, self.num_classes, &self.device)?;
        Ok(())
    }

    pub fn classes(&self) -> &[String] {
        &self.classes
    }

    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    pub fn predict(
        &self,
        text: &str,
        amount_minor: i64,
        booking_date_str: &str,
    ) -> Result<String, AppError> {
        let cleaned = text.split('/').next().unwrap_or(text).trim().to_lowercase();

        let encoding = self
            .tokenizer
            .encode(cleaned, true)
            .map_err(|e| AppError::Internal(format!("tokenisation: {e}")))?;

        let ids: Vec<u32> = encoding.get_ids().iter().copied().take(128).collect();
        let len = ids.len();

        let input_ids = Tensor::from_vec(ids, (1, len), &self.device)
            .map_err(|e| AppError::Internal(format!("input_ids tensor: {e}")))?;
        let attention_mask = Tensor::ones((1, len), candle_core::DType::U32, &self.device)
            .map_err(|e| AppError::Internal(format!("attention_mask tensor: {e}")))?;

        let features_vec = build_features(amount_minor, booking_date_str)?;
        let features = Tensor::from_vec(features_vec, (1, features::FEATURE_DIM), &self.device)
            .map_err(|e| AppError::Internal(format!("features tensor: {e}")))?;

        let text_emb = self
            .encoder
            .encode(&input_ids, &attention_mask)
            .map_err(|e| AppError::Internal(format!("encoder: {e}")))?;

        let logits = self
            .head
            .forward(&text_emb, &features)
            .map_err(|e| AppError::Internal(format!("head forward: {e}")))?;

        let logits_vec = logits
            .squeeze(0)
            .and_then(|t| t.to_vec1::<f32>())
            .map_err(|e| AppError::Internal(format!("logits to vec: {e}")))?;

        let max_idx = logits_vec
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);

        Ok(self
            .classes
            .get(max_idx)
            .cloned()
            .unwrap_or_else(|| "Other".to_string()))
    }
}

// ---------------------------------------------------------------------------
// TrainableClassifier — mutable head weights for fine-tuning
// ---------------------------------------------------------------------------

pub struct TrainableClassifier {
    pub encoder: MiniLmEncoder,
    pub head: ClassificationHead,
    pub var_map: VarMap,
    pub classes: Vec<String>,
    pub device: Device,
}

impl TrainableClassifier {
    pub fn load(
        app_data_dir: &Path,
        weights_path: &Path,
        classes: Vec<String>,
    ) -> Result<Self, AppError> {
        let device = best_device();
        let encoder = load_encoder(app_data_dir, &device)?;
        let num_classes = classes.len();

        let mut var_map = VarMap::new();
        var_map
            .load(weights_path)
            .map_err(|e| AppError::Internal(format!("seed varmap: {e}")))?;
        let vb = VarBuilder::from_varmap(&var_map, DType::F32, &device);
        let head = ClassificationHead::new(num_classes, vb)
            .map_err(|e| AppError::Internal(format!("build head: {e}")))?;

        Ok(Self {
            encoder,
            head,
            var_map,
            classes,
            device,
        })
    }

    pub fn load_fresh(app_data_dir: &Path, classes: Vec<String>) -> Result<Self, AppError> {
        let device = best_device();
        let encoder = load_encoder(app_data_dir, &device)?;
        let num_classes = classes.len();

        let var_map = VarMap::new();
        let vb = VarBuilder::from_varmap(&var_map, DType::F32, &device);
        let head = ClassificationHead::new(num_classes, vb)
            .map_err(|e| AppError::Internal(format!("build fresh head: {e}")))?;

        Ok(Self {
            encoder,
            head,
            var_map,
            classes,
            device,
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Io(format!("create model dir: {e}")))?;
        }
        self.var_map
            .save(path)
            .map_err(|e| AppError::Internal(format!("save weights: {e}")))
    }
}
