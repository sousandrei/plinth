use burn::tensor::{Int, Tensor, TensorData, backend::Backend};
use tokenizers::Tokenizer;

use crate::{db::DbPool, error::AppError};

use super::features::{FEATURE_DIM, build_features};

pub struct TransactionSample {
    /// WordPiece token ids including [CLS] and [SEP], capped at 128.
    pub input_ids: Vec<u32>,
    pub features: Vec<f32>,
    pub label: usize,
}

pub struct TokenBatch<B: Backend<IntElem = i32>> {
    /// Padded token ids — [batch, max_seq_len]
    pub input_ids: Tensor<B, 2, Int>,
    /// 1 for real tokens, 0 for padding — [batch, max_seq_len]
    pub attention_mask: Tensor<B, 2, Int>,
}

pub struct EmbeddedBatch<B: Backend<FloatElem = f32, IntElem = i32>> {
    pub embeddings: Tensor<B, 2>,
    pub features: Tensor<B, 2>,
    pub labels: Tensor<B, 2>,
}

fn clean_text(text: &str) -> String {
    text.split('/').next().unwrap_or(text).trim().to_lowercase()
}

pub async fn load_approved(
    pool: &DbPool,
    space_id: &str,
    tokenizer: &Tokenizer,
    classes: &[String],
) -> Result<Vec<TransactionSample>, AppError> {
    let rows = sqlx::query_file!("queries/training/get_approved_transactions.sql", space_id)
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

pub fn make_token_batch<B: Backend<IntElem = i32>>(
    samples: &[&TransactionSample],
    device: &B::Device,
) -> Result<TokenBatch<B>, AppError> {
    if samples.is_empty() {
        return Err(AppError::InvalidInput(
            "cannot build a token batch without samples".into(),
        ));
    }

    let batch_size = samples.len();
    let max_len = samples
        .iter()
        .map(|sample| sample.input_ids.len())
        .max()
        .unwrap_or(1)
        .max(1);
    let mut id_data = vec![0i32; batch_size * max_len];
    let mut mask_data = vec![0i32; batch_size * max_len];

    for (row_index, sample) in samples.iter().enumerate() {
        let row_start = row_index * max_len;
        for (column_index, &token_id) in sample.input_ids.iter().enumerate() {
            id_data[row_start + column_index] = i32::try_from(token_id)
                .map_err(|e| AppError::InvalidInput(format!("MiniLM token id: {e}")))?;
        }
        mask_data[row_start..row_start + sample.input_ids.len()].fill(1);
    }

    Ok(TokenBatch {
        input_ids: Tensor::from_data(TensorData::new(id_data, [batch_size, max_len]), device),
        attention_mask: Tensor::from_data(
            TensorData::new(mask_data, [batch_size, max_len]),
            device,
        ),
    })
}

/// Assemble training tensors from frozen encoder outputs and transaction labels.
pub fn make_embedded_batch<B: Backend<FloatElem = f32, IntElem = i32>>(
    embeddings: &Tensor<B, 2>,
    samples: &[TransactionSample],
    indices: &[usize],
    num_classes: usize,
    device: &B::Device,
) -> Result<EmbeddedBatch<B>, AppError> {
    let batch_size = indices.len();
    if batch_size == 0 {
        return Err(AppError::InvalidInput(
            "cannot build a training batch without samples".into(),
        ));
    }
    let mut feature_data = Vec::with_capacity(batch_size * FEATURE_DIM);
    let mut label_data = vec![0.0f32; batch_size * num_classes];
    let mut index_data = Vec::with_capacity(batch_size);

    for (position, &index) in indices.iter().enumerate() {
        if index >= embeddings.dims()[0] || index >= samples.len() {
            return Err(AppError::InvalidInput(format!(
                "training sample index {index} is out of range"
            )));
        }
        index_data.push(
            i32::try_from(index)
                .map_err(|e| AppError::InvalidInput(format!("training sample index: {e}")))?,
        );
        feature_data.extend_from_slice(&samples[index].features);
        label_data[position * num_classes + samples[index].label] = 1.0;
    }

    let embedding_indices =
        Tensor::<B, 1, Int>::from_data(TensorData::new(index_data, [batch_size]), device);
    Ok(EmbeddedBatch {
        embeddings: embeddings.clone().select(0, embedding_indices),
        features: Tensor::from_data(
            TensorData::new(feature_data, [batch_size, FEATURE_DIM]),
            device,
        ),
        labels: Tensor::from_data(
            TensorData::new(label_data, [batch_size, num_classes]),
            device,
        ),
    })
}

#[cfg(test)]
mod batch_tests {
    use super::{TransactionSample, make_embedded_batch, make_token_batch};
    use crate::classifier::backend::{InferenceBackend, default_device};
    use crate::classifier::model::TEXT_DIM;
    use burn::tensor::{Tensor, TensorData};

    #[test]
    fn builds_padded_token_batch_on_wgpu() {
        let samples = [
            TransactionSample {
                input_ids: vec![101, 12, 102],
                features: vec![],
                label: 0,
            },
            TransactionSample {
                input_ids: vec![101, 102],
                features: vec![],
                label: 1,
            },
        ];
        let sample_refs = samples.iter().collect::<Vec<_>>();
        let device = default_device();

        let batch = make_token_batch::<InferenceBackend>(&sample_refs, &device).unwrap();

        assert_eq!(batch.input_ids.dims(), [2, 3]);
        assert_eq!(
            batch.input_ids.to_data().into_vec::<i32>().unwrap(),
            [101, 12, 102, 101, 102, 0]
        );
        assert_eq!(
            batch.attention_mask.to_data().into_vec::<i32>().unwrap(),
            [1, 1, 1, 1, 1, 0]
        );
    }

    #[test]
    fn selects_embeddings_for_training_batch() {
        let samples = vec![
            TransactionSample {
                input_ids: vec![],
                features: vec![0.5; super::FEATURE_DIM],
                label: 1,
            },
            TransactionSample {
                input_ids: vec![],
                features: vec![1.5; super::FEATURE_DIM],
                label: 0,
            },
        ];
        let device = default_device();
        let mut embedding_data = vec![1.0f32; TEXT_DIM];
        embedding_data.extend(vec![2.0f32; TEXT_DIM]);
        let embeddings = Tensor::<InferenceBackend, 2>::from_data(
            TensorData::new(embedding_data, [2, TEXT_DIM]),
            &device,
        );
        let batch =
            make_embedded_batch::<InferenceBackend>(&embeddings, &samples, &[1, 0], 2, &device)
                .unwrap();

        assert_eq!(batch.embeddings.dims(), [2, TEXT_DIM]);
        assert_eq!(batch.features.dims(), [2, super::FEATURE_DIM]);
        assert_eq!(batch.labels.dims(), [2, 2]);
        assert_eq!(
            batch.embeddings.to_data().into_vec::<f32>().unwrap()[0],
            2.0
        );
        assert_eq!(batch.features.to_data().into_vec::<f32>().unwrap()[0], 1.5);
        assert_eq!(
            batch.labels.to_data().into_vec::<f32>().unwrap(),
            [1.0, 0.0, 0.0, 1.0]
        );
    }
}
