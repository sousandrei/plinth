use crate::error::AppError;
use burn::module::Module;
use burn::nn::attention::{MhaInput, MultiHeadAttention, MultiHeadAttentionConfig};
use burn::nn::{Embedding, EmbeddingConfig, LayerNorm, LayerNormConfig, Linear, LinearConfig};
use burn::store::{ModuleSnapshot, PyTorchToBurnAdapter, SafetensorsStore};
use burn::tensor::{Int, Tensor, activation, backend::Backend};
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
pub struct MiniLmConfig {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub intermediate_size: usize,
    pub max_position_embeddings: usize,
    pub type_vocab_size: usize,
    #[serde(default = "default_layer_norm_eps")]
    pub layer_norm_eps: f64,
}

fn default_layer_norm_eps() -> f64 {
    1e-12
}

#[derive(Module, Debug)]
struct BertEmbeddings<B: Backend> {
    word_embeddings: Embedding<B>,
    position_embeddings: Embedding<B>,
    token_type_embeddings: Embedding<B>,
    layer_norm: LayerNorm<B>,
}

#[derive(Module, Debug)]
struct BertSelfOutput<B: Backend> {
    layer_norm: LayerNorm<B>,
}

#[derive(Module, Debug)]
struct BertAttention<B: Backend> {
    self_attention: MultiHeadAttention<B>,
    output: BertSelfOutput<B>,
}

#[derive(Module, Debug)]
struct BertIntermediate<B: Backend> {
    dense: Linear<B>,
}

#[derive(Module, Debug)]
struct BertOutput<B: Backend> {
    dense: Linear<B>,
    layer_norm: LayerNorm<B>,
}

#[derive(Module, Debug)]
struct BertLayer<B: Backend> {
    attention: BertAttention<B>,
    intermediate: BertIntermediate<B>,
    output: BertOutput<B>,
}

#[derive(Module, Debug)]
struct BertEncoder<B: Backend> {
    layer: Vec<BertLayer<B>>,
}

#[derive(Module, Debug)]
struct Bert<B: Backend> {
    embeddings: BertEmbeddings<B>,
    encoder: BertEncoder<B>,
}

impl<B: Backend> Bert<B> {
    fn new(config: &MiniLmConfig, device: &B::Device) -> Self {
        let hidden_size = config.hidden_size;
        let layer_norm = || {
            LayerNormConfig::new(hidden_size)
                .with_epsilon(config.layer_norm_eps)
                .init(device)
        };
        let layers = (0..config.num_hidden_layers)
            .map(|_| BertLayer {
                attention: BertAttention {
                    self_attention: MultiHeadAttentionConfig::new(
                        hidden_size,
                        config.num_attention_heads,
                    )
                    .with_dropout(0.0)
                    .init(device),
                    output: BertSelfOutput {
                        layer_norm: layer_norm(),
                    },
                },
                intermediate: BertIntermediate {
                    dense: LinearConfig::new(hidden_size, config.intermediate_size).init(device),
                },
                output: BertOutput {
                    dense: LinearConfig::new(config.intermediate_size, hidden_size).init(device),
                    layer_norm: layer_norm(),
                },
            })
            .collect();

        Self {
            embeddings: BertEmbeddings {
                word_embeddings: EmbeddingConfig::new(config.vocab_size, hidden_size).init(device),
                position_embeddings: EmbeddingConfig::new(
                    config.max_position_embeddings,
                    hidden_size,
                )
                .init(device),
                token_type_embeddings: EmbeddingConfig::new(config.type_vocab_size, hidden_size)
                    .init(device),
                layer_norm: layer_norm(),
            },
            encoder: BertEncoder { layer: layers },
        }
    }

    fn forward(
        &self,
        input_ids: Tensor<B, 2, Int>,
        attention_mask: Tensor<B, 2, Int>,
    ) -> Tensor<B, 3> {
        let [batch_size, sequence_length] = input_ids.dims();
        let device = input_ids.device();
        let position_ids = Tensor::<B, 1, Int>::arange(0..sequence_length as i64, &device)
            .reshape([1, sequence_length])
            .repeat_dim(0, batch_size);
        let token_type_ids = input_ids.clone().mul_scalar(0);
        let mut hidden = self.embeddings.word_embeddings.forward(input_ids)
            + self.embeddings.position_embeddings.forward(position_ids)
            + self
                .embeddings
                .token_type_embeddings
                .forward(token_type_ids);
        hidden = self.embeddings.layer_norm.forward(hidden);

        let padding_mask = attention_mask.equal_elem(0);
        for layer in &self.encoder.layer {
            let residual = hidden.clone();
            let attended = layer
                .attention
                .self_attention
                .forward(MhaInput::self_attn(hidden).mask_pad(padding_mask.clone()))
                .context;
            hidden = layer
                .attention
                .output
                .layer_norm
                .forward(attended + residual);

            let residual = hidden.clone();
            let intermediate = activation::gelu(layer.intermediate.dense.forward(hidden));
            hidden = layer
                .output
                .layer_norm
                .forward(layer.output.dense.forward(intermediate) + residual);
        }

        hidden
    }
}

pub struct MiniLmEncoder<B: Backend> {
    bert: Bert<B>,
}

impl<B: Backend> MiniLmEncoder<B> {
    pub fn load(
        weights_path: &Path,
        config: &MiniLmConfig,
        device: &B::Device,
    ) -> std::result::Result<Self, AppError> {
        let mut bert = Bert::new(config, device);
        let mut store = SafetensorsStore::from_file(weights_path)
            .with_from_adapter(PyTorchToBurnAdapter)
            .with_key_remapping(r"^bert\.", "")
            .with_key_remapping(
                r"\.attention\.output\.dense\.",
                ".attention.self_attention.output.",
            )
            .with_key_remapping(r"\.self\.", ".self_attention.")
            .with_key_remapping(r"\.LayerNorm\.", ".layer_norm.");
        let result = bert
            .load_from(&mut store)
            .map_err(|e| AppError::Internal(format!("load MiniLM SafeTensors: {e}")))?;
        if !result.is_success() || !result.missing.is_empty() {
            return Err(AppError::Internal(format!(
                "load MiniLM weights incomplete: errors={:?}, missing={:?}",
                result.errors, result.missing
            )));
        }

        Ok(Self {
            bert: bert.no_grad(),
        })
    }

    pub fn encode(
        &self,
        input_ids: Tensor<B, 2, Int>,
        attention_mask: Tensor<B, 2, Int>,
    ) -> Tensor<B, 2> {
        let mask = attention_mask.clone().float().unsqueeze_dim(2);
        let hidden = self.bert.forward(input_ids, attention_mask);
        let sum = (hidden * mask.clone()).sum_dim(1).squeeze_dim::<2>(1);
        let count = mask.sum_dim(1).squeeze_dim::<2>(1).clamp_min(1e-9);
        let pooled = sum / count;
        let norm = pooled
            .clone()
            .powf_scalar(2.0)
            .sum_dim(1)
            .sqrt()
            .clamp_min(1e-9);
        pooled / norm
    }
}

#[cfg(test)]
mod tests {
    use super::{Bert, MiniLmConfig, MiniLmEncoder, SafetensorsStore};
    use crate::classifier::backend::{InferenceBackend, TrainingBackend, default_device};
    use burn::store::{BurnToPyTorchAdapter, ModuleSnapshot};
    use burn::tensor::{Int, Tensor, TensorData};
    #[test]
    fn minilm_encoder_runs_on_wgpu() {
        let device = default_device();
        let config = MiniLmConfig {
            vocab_size: 32,
            hidden_size: 8,
            num_hidden_layers: 1,
            num_attention_heads: 2,
            intermediate_size: 16,
            max_position_embeddings: 8,
            type_vocab_size: 2,
            layer_norm_eps: 1e-12,
        };
        let encoder = MiniLmEncoder {
            bert: Bert::new(&config, &device),
        };
        let input_ids = Tensor::<InferenceBackend, 2, Int>::from_data(
            TensorData::from([[1i32, 2, 3, 0], [4, 5, 0, 0]]),
            &device,
        );
        let attention_mask = Tensor::<InferenceBackend, 2, Int>::from_data(
            TensorData::from([[1i32, 1, 1, 0], [1, 1, 0, 0]]),
            &device,
        );

        let embeddings = encoder.encode(input_ids, attention_mask);
        let values = embeddings.to_data().into_vec::<f32>().unwrap();

        assert_eq!(embeddings.dims(), [2, 8]);
        for row in values.chunks_exact(8) {
            let norm = row.iter().map(|value| value * value).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn minilm_encoder_loads_hugging_face_safetensors() {
        let device = default_device();
        let config = MiniLmConfig {
            vocab_size: 32,
            hidden_size: 8,
            num_hidden_layers: 1,
            num_attention_heads: 2,
            intermediate_size: 16,
            max_position_embeddings: 8,
            type_vocab_size: 2,
            layer_norm_eps: 1e-12,
        };
        let source = MiniLmEncoder {
            bert: Bert::new(&config, &device),
        };
        let path =
            std::env::temp_dir().join(format!("plinth-minilm-{}.safetensors", std::process::id()));
        let input_ids = Tensor::<InferenceBackend, 2, Int>::from_data(
            TensorData::from([[1i32, 2, 3, 0], [4, 5, 0, 0]]),
            &device,
        );
        let attention_mask = Tensor::<InferenceBackend, 2, Int>::from_data(
            TensorData::from([[1i32, 1, 1, 0], [1, 1, 0, 0]]),
            &device,
        );
        let expected = source
            .encode(input_ids.clone(), attention_mask.clone())
            .to_data()
            .into_vec::<f32>()
            .unwrap();
        let mut store = SafetensorsStore::from_file(&path)
            .with_to_adapter(BurnToPyTorchAdapter)
            .with_key_remapping(r"\.self_attention\.output\.", ".output.dense.")
            .with_key_remapping(r"\.self_attention\.", ".self.")
            .with_key_remapping(r"\.layer_norm\.", ".LayerNorm.");
        source.bert.save_into(&mut store).unwrap();

        let restored = MiniLmEncoder::<InferenceBackend>::load(&path, &config, &device).unwrap();
        let actual = restored
            .encode(input_ids, attention_mask)
            .to_data()
            .into_vec::<f32>()
            .unwrap();

        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-5);
        }

        let train_encoder =
            MiniLmEncoder::<TrainingBackend>::load(&path, &config, &device).unwrap();
        let train_input_ids = Tensor::<TrainingBackend, 2, Int>::from_data(
            TensorData::from([[1i32, 2, 3, 0], [4, 5, 0, 0]]),
            &device,
        );
        let train_attention_mask = Tensor::<TrainingBackend, 2, Int>::from_data(
            TensorData::from([[1i32, 1, 1, 0], [1, 1, 0, 0]]),
            &device,
        );
        let train_embeddings = train_encoder.encode(train_input_ids, train_attention_mask);
        assert!(!train_embeddings.is_require_grad());

        std::fs::remove_file(path).unwrap();
    }
}
