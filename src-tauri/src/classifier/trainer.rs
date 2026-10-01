use burn::optim::{GradientsParams, Optimizer};
use burn::tensor::{Tensor, activation, backend::AutodiffBackend};

use super::backend::{Device, TrainingBackend};
use crate::error::AppError;

use super::dataset::{TransactionSample, make_embedded_batch, make_token_batch};
use super::encoder::MiniLmEncoder;
use super::model::{ClassificationHead, TEXT_DIM};

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

#[allow(clippy::too_many_arguments)]
pub fn run_epoch<B, O>(
    head: ClassificationHead<B>,
    optimizer: &mut O,
    embeddings: &Tensor<B, 2>,
    samples: &[TransactionSample],
    split: &DataSplit,
    config: &TrainingConfig,
    epoch: u32,
    num_classes: usize,
    device: &B::Device,
) -> Result<(ClassificationHead<B>, EpochResult), AppError>
where
    B: AutodiffBackend<FloatElem = f32, IntElem = i32>,
    O: Optimizer<ClassificationHead<B>, B>,
{
    let epoch_lr = {
        let t = (epoch - 1) as f64;
        let t_max = config.epochs as f64;
        config.learning_rate * 0.5 * (1.0 + (std::f64::consts::PI * t / t_max).cos())
    };

    let mut train_indices = split.train.clone();
    let mut rng = (epoch as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    for i in (1..train_indices.len()).rev() {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (rng >> 33) as usize % (i + 1);
        train_indices.swap(i, j);
    }

    let (head, train_loss, train_accuracy) = forward_pass(
        head,
        optimizer,
        embeddings,
        samples,
        &train_indices,
        config.batch_size,
        num_classes,
        device,
        epoch_lr,
        true,
    )?;
    let (head, val_loss, val_accuracy) = forward_pass(
        head,
        optimizer,
        embeddings,
        samples,
        &split.val,
        config.batch_size,
        num_classes,
        device,
        epoch_lr,
        false,
    )?;

    Ok((
        head,
        EpochResult {
            epoch,
            train_loss,
            train_accuracy,
            val_loss,
            val_accuracy,
        },
    ))
}

#[allow(clippy::too_many_arguments)]
fn forward_pass<B, O>(
    mut head: ClassificationHead<B>,
    optimizer: &mut O,
    embeddings: &Tensor<B, 2>,
    samples: &[TransactionSample],
    indices: &[usize],
    batch_size: usize,
    num_classes: usize,
    device: &B::Device,
    learning_rate: f64,
    train: bool,
) -> Result<(ClassificationHead<B>, f32, f32), AppError>
where
    B: AutodiffBackend<FloatElem = f32, IntElem = i32>,
    O: Optimizer<ClassificationHead<B>, B>,
{
    if indices.is_empty() {
        return Ok((head, 0.0, 0.0));
    }

    let mut total_loss = 0.0;
    let mut total_correct = 0usize;
    let mut total_count = 0usize;
    let mut num_batches = 0usize;

    for chunk in indices.chunks(batch_size) {
        let batch = make_embedded_batch::<B>(embeddings, samples, chunk, num_classes, device)?;
        let logits = head.forward(batch.embeddings, batch.features);
        let log_probs = activation::log_softmax(logits.clone(), 1);
        let loss = (batch.labels.clone() * log_probs)
            .sum()
            .mul_scalar(-1.0 / chunk.len() as f64);
        let loss_value = loss.clone().into_scalar();
        if !loss_value.is_finite() {
            continue;
        }

        let predictions = logits
            .to_data()
            .into_vec::<f32>()
            .map_err(|e| AppError::Internal(format!("logits to data: {e}")))?;
        let targets = batch
            .labels
            .to_data()
            .into_vec::<f32>()
            .map_err(|e| AppError::Internal(format!("labels to data: {e}")))?;
        for row in 0..chunk.len() {
            let row_start = row * num_classes;
            let predicted = argmax_slice(&predictions[row_start..row_start + num_classes]);
            let target = argmax_slice(&targets[row_start..row_start + num_classes]);
            if predicted == target {
                total_correct += 1;
            }
        }

        if train {
            let grads = GradientsParams::from_grads(loss.backward(), &head);
            head = optimizer.step(learning_rate, head, grads);
        }
        total_loss += loss_value;
        total_count += chunk.len();
        num_batches += 1;
    }

    if num_batches == 0 {
        return Ok((head, 0.0, 0.0));
    }
    Ok((
        head,
        total_loss / num_batches as f32,
        total_correct as f32 / total_count as f32,
    ))
}

fn argmax_slice(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

#[cfg(test)]
fn train_step<B: AutodiffBackend<FloatElem = f32>>(
    head: ClassificationHead<B>,
    optimizer: &mut impl Optimizer<ClassificationHead<B>, B>,
    text_embeddings: Tensor<B, 2>,
    features: Tensor<B, 2>,
    labels: Tensor<B, 2>,
    learning_rate: f64,
) -> Result<(ClassificationHead<B>, f32), AppError> {
    let logits = head.forward(text_embeddings, features);
    let log_probs = activation::log_softmax(logits, 1);
    let batch_size = labels.dims()[0] as f64;
    let loss = (labels * log_probs).sum().mul_scalar(-1.0 / batch_size);
    let loss_value = loss.clone().into_scalar();
    if !loss_value.is_finite() {
        return Err(AppError::Internal("loss is not finite".into()));
    }

    let grads = GradientsParams::from_grads(loss.backward(), &head);
    let head = optimizer.step(learning_rate, head, grads);
    Ok((head, loss_value))
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

/// Run the frozen MiniLM encoder once before the epoch loop.
/// Returns one [384] embedding row per sample, in sample order.
///
/// `progress` is invoked after each encoder batch with the cumulative count
/// of embedded samples and the total, so the caller can surface progress
/// to the UI during long precompute passes.
pub fn precompute_embeddings<F>(
    encoder: &MiniLmEncoder<TrainingBackend>,
    samples: &[TransactionSample],
    batch_size: usize,
    device: &Device,
    mut progress: F,
) -> Result<Tensor<TrainingBackend, 2>, AppError>
where
    F: FnMut(usize, usize),
{
    if batch_size == 0 {
        return Err(AppError::InvalidInput(
            "embedding batch size must be greater than zero".into(),
        ));
    }
    if samples.is_empty() {
        return Err(AppError::InvalidInput(
            "cannot precompute embeddings without samples".into(),
        ));
    }

    let mut embedding_batches = Vec::with_capacity(samples.len().div_ceil(batch_size));
    let total = samples.len();

    for chunk in samples.chunks(batch_size) {
        let batch_refs: Vec<&TransactionSample> = chunk.iter().collect();
        let batch = make_token_batch::<TrainingBackend>(&batch_refs, device)?;
        let embeddings = encoder
            .encode(batch.input_ids, batch.attention_mask)
            .detach();
        if embeddings.dims() != [chunk.len(), TEXT_DIM] {
            return Err(AppError::Internal(format!(
                "encoder precompute returned {:?} values for {} samples",
                embeddings.dims(),
                chunk.len()
            )));
        }
        embedding_batches.push(embeddings);

        progress(
            (embedding_batches.len() - 1) * batch_size + chunk.len(),
            total,
        );
    }

    Ok(Tensor::cat(embedding_batches, 0))
}

#[cfg(test)]
mod training_tests {
    use super::{DataSplit, TrainingConfig, run_epoch, train_step};
    use crate::classifier::TrainingBackend;
    use crate::classifier::backend::default_device;
    use crate::classifier::dataset::TransactionSample;
    use crate::classifier::model::{ClassificationHead, NUMERIC_DIM, TEXT_DIM};
    use burn::optim::AdamWConfig;
    use burn::tensor::{Tensor, TensorData};

    #[test]
    fn optimizer_reduces_classification_loss_on_wgpu() {
        let device = default_device();
        let mut head = ClassificationHead::<TrainingBackend>::new(2, &device);
        let mut optimizer =
            AdamWConfig::new().init::<TrainingBackend, ClassificationHead<TrainingBackend>>();
        let text = Tensor::<TrainingBackend, 2>::ones([4, TEXT_DIM], &device);
        let mut feature_data = vec![0.0; 4 * NUMERIC_DIM];
        feature_data[2 * NUMERIC_DIM..].fill(1.0);
        let features = Tensor::<TrainingBackend, 2>::from_data(
            TensorData::new(feature_data, [4, NUMERIC_DIM]),
            &device,
        );
        let labels = Tensor::from_floats([[1.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, 1.0]], &device);
        let mut initial_loss = 0.0;
        let mut final_loss = 0.0;

        for step in 0..12 {
            let (updated, loss) = train_step(
                head,
                &mut optimizer,
                text.clone(),
                features.clone(),
                labels.clone(),
                1e-2,
            )
            .unwrap();
            head = updated;
            if step == 0 {
                initial_loss = loss;
            }
            final_loss = loss;
        }

        assert!(
            final_loss < initial_loss,
            "loss did not decrease: {initial_loss} -> {final_loss}"
        );
    }

    #[test]
    fn epoch_trains_and_validates_on_wgpu() {
        let device = default_device();
        let embedding_data = (0..4)
            .flat_map(|index| vec![index as f32 / 4.0; TEXT_DIM])
            .collect::<Vec<_>>();
        let embeddings = Tensor::<TrainingBackend, 2>::from_data(
            TensorData::new(embedding_data, [4, TEXT_DIM]),
            &device,
        );
        let samples = (0..4)
            .map(|index| TransactionSample {
                input_ids: vec![],
                features: vec![index as f32 / 4.0; NUMERIC_DIM],
                label: index % 2,
            })
            .collect::<Vec<_>>();
        let split = DataSplit {
            train: vec![0, 1, 2, 3],
            val: vec![0, 1],
        };
        let config = TrainingConfig {
            epochs: 1,
            batch_size: 2,
            learning_rate: 1e-2,
            weight_decay: 1e-4,
        };
        let head = ClassificationHead::<TrainingBackend>::new(2, &device);
        let mut optimizer =
            AdamWConfig::new().init::<TrainingBackend, ClassificationHead<TrainingBackend>>();

        let (head, result) = run_epoch(
            head,
            &mut optimizer,
            &embeddings,
            &samples,
            &split,
            &config,
            1,
            2,
            &device,
        )
        .unwrap();

        assert_eq!(result.epoch, 1);
        assert!(result.train_loss.is_finite());
        assert!(result.val_loss.is_finite());
        assert!((0.0..=1.0).contains(&result.train_accuracy));
        assert!((0.0..=1.0).contains(&result.val_accuracy));
        assert_eq!(
            head.forward(
                Tensor::zeros([1, TEXT_DIM], &device),
                Tensor::zeros([1, NUMERIC_DIM], &device)
            )
            .dims(),
            [1, 2]
        );
    }
}
