use std::path::Path;

use crate::error::AppError;

use super::assets::load_encoder;
use super::backend::{Device, TrainingBackend, default_device};
use super::encoder::MiniLmEncoder;
use super::model::ClassificationHead;

pub struct TrainableClassifier {
    pub encoder: MiniLmEncoder<TrainingBackend>,
    pub head: ClassificationHead<TrainingBackend>,
    pub classes: Vec<String>,
    pub device: Device,
}

impl TrainableClassifier {
    pub fn load_fresh(app_data_dir: &Path, classes: Vec<String>) -> Result<Self, AppError> {
        let device = default_device();
        let encoder = load_encoder::<TrainingBackend>(app_data_dir, &device)?;
        let head = ClassificationHead::new(classes.len(), &device);

        Ok(Self {
            encoder,
            head,
            classes,
            device,
        })
    }
}
