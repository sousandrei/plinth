use candle_core::{Device, Tensor};
use candle_nn::{AdamW, Optimizer, ParamsAdamW, VarMap};

use crate::error::AppError;

use super::dataset::{TransactionSample, make_embedded_batch, make_token_batch};
use super::model::{ClassificationHead, MiniLmEncoder};

pub struct TrainingConfig {
    pub epochs: u32,
    pub batch_size: usize,
    pub learning_rate: f64,
    pub weight_decay: f64,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            epochs: 10,
            batch_size: 32,
            learning_rate: 1e-3,
            weight_decay: 1e-4,
        }
    }
}

pub struct EpochResult {
    pub epoch: u32,
    pub train_loss: f32,
    pub train_accuracy: f32,
    pub val_loss: f32,
    pub val_accuracy: f32,
}

pub struct DataSplit {
    pub train: Vec<usize>,
    pub val: Vec<usize>,
}

pub fn split_indices(num_samples: usize, train_ratio: f32) -> DataSplit {
    let mut indices: Vec<usize> = (0..num_samples).collect();
    let mut rng = 0xdeadbeef_cafebabe_u64;
    for i in (1..indices.len()).rev() {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (rng >> 33) as usize % (i + 1);
        indices.swap(i, j);
    }
    let split = ((num_samples as f32 * train_ratio) as usize)
        .max(1)
        .min(num_samples - 1);
    let val = indices.split_off(split);
    DataSplit {
        train: indices,
        val,
    }
}

/// Run the frozen MiniLM encoder once over all samples before the epoch loop.
/// Returns one [384] embedding tensor per sample, in sample order.
///
/// `progress` is invoked after each encoder batch with the cumulative count
/// of embedded samples and the total, so the caller can surface progress
/// to the UI during long precompute passes.
pub fn precompute_embeddings<F>(
    encoder: &MiniLmEncoder,
    samples: &[TransactionSample],
    batch_size: usize,
    device: &Device,
    mut progress: F,
) -> Result<Vec<Tensor>, AppError>
where
    F: FnMut(usize, usize),
{
    let mut embeddings: Vec<Tensor> = Vec::with_capacity(samples.len());
    let total = samples.len();

    for chunk in samples.chunks(batch_size) {
        let batch_refs: Vec<&TransactionSample> = chunk.iter().collect();
        let batch = make_token_batch(&batch_refs, device)?;

        let emb = encoder
            .encode(&batch.input_ids, &batch.attention_mask)
            .map_err(|e| AppError::Internal(format!("encoder precompute: {e}")))?;

        for i in 0..chunk.len() {
            embeddings.push(
                emb.get(i)
                    .map_err(|e| AppError::Internal(format!("embedding row {i}: {e}")))?,
            );
        }

        progress(embeddings.len(), total);
    }

    Ok(embeddings)
}

pub fn make_optimizer(var_map: &VarMap, config: &TrainingConfig) -> Result<AdamW, AppError> {
    AdamW::new(
        var_map.all_vars(),
        ParamsAdamW {
            lr: config.learning_rate,
            weight_decay: config.weight_decay,
            ..ParamsAdamW::default()
        },
    )
    .map_err(|e| AppError::Internal(format!("optimizer init: {e}")))
}

/// Run one epoch using pre-computed embeddings. The encoder is not called here.
#[allow(clippy::too_many_arguments)]
pub fn run_epoch(
    embeddings: &[Tensor],
    head: &ClassificationHead,
    optimizer: &mut AdamW,
    var_map: &VarMap,
    samples: &[TransactionSample],
    split: &DataSplit,
    config: &TrainingConfig,
    epoch: u32,
    num_classes: usize,
    device: &Device,
) -> Result<EpochResult, AppError> {
    // Cosine annealing.
    let cos_lr = {
        let t = (epoch - 1) as f64;
        let t_max = config.epochs as f64;
        config.learning_rate * 0.5 * (1.0 + (std::f64::consts::PI * t / t_max).cos())
    };
    optimizer.set_learning_rate(cos_lr);

    // Per-epoch shuffle of training indices only.
    let mut train_idx = split.train.clone();
    let mut rng = (epoch as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    for i in (1..train_idx.len()).rev() {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (rng >> 33) as usize % (i + 1);
        train_idx.swap(i, j);
    }

    let (train_loss, train_accuracy) = forward_pass(
        embeddings,
        head,
        optimizer,
        var_map,
        samples,
        &train_idx,
        num_classes,
        config.batch_size,
        device,
        true,
    )?;

    let (val_loss, val_accuracy) = forward_pass(
        embeddings,
        head,
        optimizer,
        var_map,
        samples,
        &split.val,
        num_classes,
        config.batch_size,
        device,
        false,
    )?;

    Ok(EpochResult {
        epoch,
        train_loss,
        train_accuracy,
        val_loss,
        val_accuracy,
    })
}

const MAX_GRAD_NORM: f64 = 0.1;

#[allow(clippy::too_many_arguments)]
fn forward_pass(
    embeddings: &[Tensor],
    head: &ClassificationHead,
    optimizer: &mut AdamW,
    var_map: &VarMap,
    samples: &[TransactionSample],
    indices: &[usize],
    num_classes: usize,
    batch_size: usize,
    device: &Device,
    train: bool,
) -> Result<(f32, f32), AppError> {
    if indices.is_empty() {
        return Ok((0.0, 0.0));
    }

    let mut total_loss = 0f32;
    let mut total_correct = 0usize;
    let mut total_count = 0usize;
    let mut num_batches = 0usize;

    for chunk in indices.chunks(batch_size) {
        let batch = make_embedded_batch(embeddings, samples, chunk, num_classes, device)?;

        let logits = head
            .forward(&batch.embeddings, &batch.features)
            .map_err(|e| AppError::Internal(format!("head forward: {e}")))?;

        let loss = soft_cross_entropy(&logits, &batch.labels)
            .map_err(|e| AppError::Internal(format!("loss: {e}")))?;

        if train {
            let mut grads = loss
                .backward()
                .map_err(|e| AppError::Internal(format!("backward: {e}")))?;

            let vars = var_map.all_vars();
            let mut norm_sq = 0f64;
            let mut corrupt = false;
            for v in &vars {
                if let Some(g) = grads.get(v.as_tensor()) {
                    match g
                        .sqr()
                        .and_then(|t| t.sum_all())
                        .and_then(|t| t.to_scalar::<f32>())
                    {
                        Ok(sq) if sq.is_finite() => norm_sq += sq as f64,
                        _ => {
                            corrupt = true;
                            break;
                        }
                    }
                }
            }

            if corrupt {
                continue;
            }

            let global_norm = norm_sq.sqrt();
            if global_norm > MAX_GRAD_NORM {
                let scale = MAX_GRAD_NORM / (global_norm + 1e-6);
                for v in &vars {
                    if let Some(g) = grads.remove(v.as_tensor())
                        && let Ok(scaled) = g.affine(scale, 0.0)
                    {
                        grads.insert(v.as_tensor(), scaled);
                    }
                }
            }

            optimizer
                .step(&grads)
                .map_err(|e| AppError::Internal(format!("step: {e}")))?;
        }

        let loss_val = loss
            .to_scalar::<f32>()
            .map_err(|e| AppError::Internal(format!("loss scalar: {e}")))?;

        if !loss_val.is_finite() {
            continue;
        }

        total_loss += loss_val;

        let pred = argmax_rows(&logits)?;
        let target = argmax_rows(&batch.labels)?;
        total_correct += pred
            .iter()
            .zip(target.iter())
            .filter(|(p, t)| p == t)
            .count();
        total_count += chunk.len();
        num_batches += 1;
    }

    if num_batches == 0 {
        return Ok((0.0, 0.0));
    }

    Ok((
        total_loss / num_batches as f32,
        total_correct as f32 / total_count as f32,
    ))
}

fn soft_cross_entropy(logits: &Tensor, targets: &Tensor) -> candle_core::Result<Tensor> {
    let log_probs = candle_nn::ops::log_softmax(logits, 1)?;
    let batch_size = logits.dim(0)? as f64;
    (targets * log_probs)?
        .sum_all()?
        .affine(-1.0 / batch_size, 0.0)
}

fn argmax_rows(t: &Tensor) -> Result<Vec<usize>, AppError> {
    t.to_vec2::<f32>()
        .map_err(|e| AppError::Internal(format!("argmax to_vec2: {e}")))?
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .ok_or_else(|| AppError::Internal("empty row in argmax".into()))
        })
        .collect()
}
