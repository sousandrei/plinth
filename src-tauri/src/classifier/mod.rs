pub mod backend;
pub mod dataset;
pub mod encoder;
pub mod features;
pub mod inference;
pub mod model;
pub mod trainable;
pub mod trainer;

mod assets;

pub use backend::TrainingBackend;
pub use inference::Classifier;
pub use trainable::TrainableClassifier;
