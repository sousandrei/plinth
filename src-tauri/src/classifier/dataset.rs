use candle_core::{Device, Tensor};
use tokenizers::Tokenizer;

use crate::{db::DbPool, error::AppError};

use super::features::{build_features, FEATURE_DIM};

pub struct TransactionSample {
    /// WordPiece token ids including [CLS] and [SEP], capped at 128.
    pub input_ids: Vec<u32>,
    pub features: Vec<f32>,
    pub label: usize,
}

pub struct TokenBatch {
    /// Padded token ids — [batch, max_seq_len]
    pub input_ids: Tensor,
    /// 1 for real tokens, 0 for padding — [batch, max_seq_len]
    pub attention_mask: Tensor,
    /// Stacked numeric feature vectors — [batch, FEATURE_DIM]
    pub features: Tensor,
    /// One-hot encoded labels — [batch, num_classes]
    pub labels: Tensor,
}

pub struct EmbeddedBatch {
    /// Stacked pre-computed embeddings — [batch, 384]
    pub embeddings: Tensor,
    /// Stacked numeric feature vectors — [batch, FEATURE_DIM]
    pub features: Tensor,
    /// One-hot encoded labels — [batch, num_classes]
    pub labels: Tensor,
}

fn clean_text(text: &str) -> String {
    text.split('/').next().unwrap_or(text).trim().to_lowercase()
}

pub async fn load_approved(
    pool: &DbPool,
    user_id: &str,
    tokenizer: &Tokenizer,
    classes: &[String],
) -> Result<Vec<TransactionSample>, AppError> {
    let rows = sqlx::query_file!("queries/training/get_approved_transactions.sql", user_id)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Db(format!("load_approved: {e}")))?;

    let mut samples = Vec::with_capacity(rows.len());

    for row in rows {
        let text = clean_text(&row.text);

        // MiniLM max is 512; cap at 128 to keep batches fast.
        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| AppError::Internal(format!("tokenisation: {e}")))?;
        let ids: Vec<u32> = encoding.get_ids().iter().copied().take(128).collect();

        let features = build_features(row.amount, &row.booking_date)?;

        let label = classes
            .iter()
            .position(|c| c == &row.category)
            .or_else(|| classes.iter().position(|c| c == "Other"))
            .ok_or_else(|| AppError::Internal("no 'Other' class".into()))?;

        samples.push(TransactionSample {
            input_ids: ids,
            features,
            label,
        });
    }

    Ok(samples)
}

/// Build a padded token batch. Used by `precompute_embeddings` to feed the encoder.
pub fn make_token_batch(
    samples: &[&TransactionSample],
    num_classes: usize,
    device: &Device,
) -> Result<TokenBatch, AppError> {
    let batch_size = samples.len();

    let max_len = samples
        .iter()
        .map(|s| s.input_ids.len())
        .max()
        .unwrap_or(1)
        .max(1);

    let mut id_data = vec![0u32; batch_size * max_len];
    let mut mask_data = vec![0u32; batch_size * max_len];

    for (i, sample) in samples.iter().enumerate() {
        let len = sample.input_ids.len().min(max_len);
        let row = i * max_len;
        id_data[row..row + len].copy_from_slice(&sample.input_ids[..len]);
        mask_data[row..row + len].fill(1);
    }

    let input_ids = Tensor::from_vec(id_data, (batch_size, max_len), device)
        .map_err(|e| AppError::Internal(format!("token_batch input_ids: {e}")))?;
    let attention_mask = Tensor::from_vec(mask_data, (batch_size, max_len), device)
        .map_err(|e| AppError::Internal(format!("token_batch attention_mask: {e}")))?;

    let feature_data: Vec<f32> = samples
        .iter()
        .flat_map(|s| s.features.iter().copied())
        .collect();
    let features = Tensor::from_vec(feature_data, (batch_size, FEATURE_DIM), device)
        .map_err(|e| AppError::Internal(format!("token_batch features: {e}")))?;

    let mut label_data = vec![0f32; batch_size * num_classes];
    for (i, sample) in samples.iter().enumerate() {
        label_data[i * num_classes + sample.label] = 1.0;
    }
    let labels = Tensor::from_vec(label_data, (batch_size, num_classes), device)
        .map_err(|e| AppError::Internal(format!("token_batch labels: {e}")))?;

    Ok(TokenBatch {
        input_ids,
        attention_mask,
        features,
        labels,
    })
}

/// Build a batch from pre-computed per-sample embeddings (indexed by `indices`).
pub fn make_embedded_batch(
    embeddings: &[Tensor],
    samples: &[TransactionSample],
    indices: &[usize],
    num_classes: usize,
    device: &Device,
) -> Result<EmbeddedBatch, AppError> {
    let batch_size = indices.len();

    let emb_refs: Vec<&Tensor> = indices.iter().map(|&i| &embeddings[i]).collect();
    let embeddings_t = Tensor::stack(&emb_refs, 0)
        .map_err(|e| AppError::Internal(format!("stack embeddings: {e}")))?;

    let feature_data: Vec<f32> = indices
        .iter()
        .flat_map(|&i| samples[i].features.iter().copied())
        .collect();
    let features = Tensor::from_vec(feature_data, (batch_size, FEATURE_DIM), device)
        .map_err(|e| AppError::Internal(format!("embedded_batch features: {e}")))?;

    let mut label_data = vec![0f32; batch_size * num_classes];
    for (pos, &i) in indices.iter().enumerate() {
        label_data[pos * num_classes + samples[i].label] = 1.0;
    }
    let labels = Tensor::from_vec(label_data, (batch_size, num_classes), device)
        .map_err(|e| AppError::Internal(format!("embedded_batch labels: {e}")))?;

    Ok(EmbeddedBatch {
        embeddings: embeddings_t,
        features,
        labels,
    })
}
