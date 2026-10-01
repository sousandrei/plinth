use crate::error::AppError;
use burn::module::Module;
use burn::nn::{Linear, LinearConfig};
use burn::store::{BurnpackStore, ModuleSnapshot};
use burn::tensor::{Tensor, activation, backend::Backend};
use std::path::Path;

pub const NUMERIC_DIM: usize = 64;
pub const TEXT_DIM: usize = 384; // MiniLM hidden size
const FUSE_IN: usize = TEXT_DIM + NUMERIC_DIM; // 448
const FUSE_HIDDEN: usize = 128;
const HEAD_DIM: usize = 32;

#[derive(Module, Debug)]
pub struct ClassificationHead<B: Backend> {
    num_fc1: Linear<B>,
    num_fc2: Linear<B>,
    fuse1: Linear<B>,
    fuse2: Linear<B>,
    head: Linear<B>,
}

impl<B: Backend> ClassificationHead<B> {
    pub fn new(num_classes: usize, device: &B::Device) -> Self {
        Self {
            num_fc1: LinearConfig::new(NUMERIC_DIM, NUMERIC_DIM).init(device),
            num_fc2: LinearConfig::new(NUMERIC_DIM, NUMERIC_DIM).init(device),
            fuse1: LinearConfig::new(FUSE_IN, FUSE_HIDDEN).init(device),
            fuse2: LinearConfig::new(FUSE_HIDDEN, HEAD_DIM).init(device),
            head: LinearConfig::new(HEAD_DIM, num_classes).init(device),
        }
    }

    pub fn forward(&self, text_emb: Tensor<B, 2>, params: Tensor<B, 2>) -> Tensor<B, 2> {
        let numeric = activation::relu(self.num_fc1.forward(params));
        let numeric = activation::relu(self.num_fc2.forward(numeric));
        let combined = Tensor::cat(vec![text_emb, numeric], 1);
        let fused = activation::relu(self.fuse1.forward(combined));
        let fused = activation::relu(self.fuse2.forward(fused));
        let norm = fused
            .clone()
            .powf_scalar(2.0)
            .sum_dim(1)
            .sqrt()
            .clamp_min(1e-9);
        self.head.forward(fused / norm)
    }

    pub fn save_weights(&self, path: &Path) -> std::result::Result<(), AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Io(format!("create model dir: {e}")))?;
        }

        let mut store = BurnpackStore::from_file(path).overwrite(true);
        self.save_into(&mut store)
            .map_err(|e| AppError::Internal(format!("save classifier head: {e}")))
    }

    pub fn load_weights(mut self, path: &Path) -> std::result::Result<Self, AppError> {
        let mut store = BurnpackStore::from_file(path).zero_copy(true);
        let result = self
            .load_from(&mut store)
            .map_err(|e| AppError::Internal(format!("load classifier head: {e}")))?;
        if !result.is_success() || !result.missing.is_empty() {
            return Err(AppError::Internal(format!(
                "load classifier head incomplete: errors={:?}, missing={:?}",
                result.errors, result.missing
            )));
        }

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{ClassificationHead, NUMERIC_DIM, TEXT_DIM};
    use crate::classifier::backend::{InferenceBackend, TrainingBackend, default_device};
    use burn::tensor::Tensor;
    #[test]
    fn classification_head_runs_on_wgpu() {
        let device = default_device();
        let head = ClassificationHead::<TrainingBackend>::new(3, &device);
        let text = Tensor::zeros([2, TEXT_DIM], &device);
        let features = Tensor::zeros([2, NUMERIC_DIM], &device);
        let logits = head.forward(text, features);

        assert_eq!(logits.dims(), [2, 3]);
    }

    #[test]
    fn classification_head_round_trips_weights() {
        let device = default_device();
        let path = std::env::temp_dir().join(format!("plinth-head-{}.bpk", std::process::id()));
        let head = ClassificationHead::<InferenceBackend>::new(3, &device);
        let text = Tensor::<InferenceBackend, 2>::ones([1, TEXT_DIM], &device);
        let features = Tensor::<InferenceBackend, 2>::ones([1, NUMERIC_DIM], &device);
        let expected = head
            .forward(text.clone(), features.clone())
            .to_data()
            .into_vec::<f32>()
            .unwrap();

        head.save_weights(&path).unwrap();
        let restored = ClassificationHead::<InferenceBackend>::new(3, &device)
            .load_weights(&path)
            .unwrap();
        let actual = restored
            .forward(text, features)
            .to_data()
            .into_vec::<f32>()
            .unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(actual, expected);
    }
}
