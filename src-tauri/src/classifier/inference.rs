use std::path::Path;

use burn::tensor::{Int, Tensor, TensorData};

use crate::error::AppError;

use super::assets::{load_encoder, load_tokenizer, minilm_dir};
use super::backend::{Device, InferenceBackend, default_device};
use super::encoder::MiniLmEncoder;
use super::features::{self, build_features};
use super::model::ClassificationHead;

fn load_head(
    weights_path: &Path,
    num_classes: usize,
    device: &Device,
) -> Result<ClassificationHead<InferenceBackend>, AppError> {
    ClassificationHead::new(num_classes, device).load_weights(weights_path)
}

pub struct Classifier {
    encoder: MiniLmEncoder<InferenceBackend>,
    head: ClassificationHead<InferenceBackend>,
    tokenizer: tokenizers::Tokenizer,
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
        let device = default_device();
        let encoder = load_encoder::<InferenceBackend>(app_data_dir, &device)?;
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

    pub fn tokenizer(&self) -> &tokenizers::Tokenizer {
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

        let ids: Vec<i32> = encoding
            .get_ids()
            .iter()
            .copied()
            .take(128)
            .map(|id| id as i32)
            .collect();
        let len = ids.len();

        let input_ids = Tensor::<InferenceBackend, 2, Int>::from_data(
            TensorData::new(ids, [1, len]),
            &self.device,
        );
        let attention_mask = Tensor::<InferenceBackend, 2, Int>::from_data(
            TensorData::new(vec![1i32; len], [1, len]),
            &self.device,
        );

        let features_vec = build_features(amount_minor, booking_date_str)?;
        let text_emb = self.encoder.encode(input_ids, attention_mask);
        let features = Tensor::<InferenceBackend, 2>::from_data(
            TensorData::new(features_vec, [1, features::FEATURE_DIM]),
            &self.device,
        );

        let logits = self.head.forward(text_emb, features);

        let logits_vec = logits
            .to_data()
            .into_vec::<f32>()
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
