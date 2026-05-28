use candle_core::{DType, Device, Module, Result, Tensor};
use candle_nn::{linear, Linear, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config as BertConfig};

pub const NUMERIC_DIM: usize = 64;
pub const TEXT_DIM: usize = 384; // MiniLM hidden size
const FUSE_IN: usize = TEXT_DIM + NUMERIC_DIM; // 448
const FUSE_HIDDEN: usize = 128;
const HEAD_DIM: usize = 32;

/// Frozen MiniLM encoder. Loaded from the minilm cache dir; its weights are
/// never added to the trainable VarMap.
pub struct MiniLmEncoder {
    bert: BertModel,
}

impl MiniLmEncoder {
    pub fn load(
        weights_path: &std::path::Path,
        config: &BertConfig,
        device: &Device,
    ) -> Result<Self> {
        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, device)? };
        let bert = BertModel::load(vb, config)?;
        Ok(Self { bert })
    }

    /// Returns mean-pooled, L2-normalised sentence embeddings: [batch, 384].
    pub fn encode(&self, input_ids: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        let token_type_ids = input_ids.zeros_like()?;
        // BertModel::forward returns [batch, seq_len, 384]
        let hidden = self
            .bert
            .forward(input_ids, &token_type_ids, Some(attention_mask))?;

        // Mean pool over non-padding positions.
        let mask = attention_mask.to_dtype(DType::F32)?.unsqueeze(2)?; // [batch, seq, 1]
        let sum = hidden.broadcast_mul(&mask)?.sum(1)?; // [batch, 384]
        let count = mask.sum(1)?.clamp(1e-9_f64, f64::MAX)?; // [batch, 1]
        let pooled = sum.broadcast_div(&count)?; // [batch, 384]

        // L2 normalise.
        let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?;
        pooled.broadcast_div(&norm)
    }
}

/// Trainable classification head: numeric MLP + fusion + classifier.
/// Only these weights live in the VarMap and are fine-tuned.
pub struct ClassificationHead {
    num_fc1: Linear, // 64 → 64
    num_fc2: Linear, // 64 → 64
    fuse1: Linear,   // 448 → 128
    fuse2: Linear,   // 128 → 32
    head: Linear,    // 32 → num_classes
}

impl ClassificationHead {
    pub fn new(num_classes: usize, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            num_fc1: linear(NUMERIC_DIM, NUMERIC_DIM, vb.pp("num_fc1"))?,
            num_fc2: linear(NUMERIC_DIM, NUMERIC_DIM, vb.pp("num_fc2"))?,
            fuse1: linear(FUSE_IN, FUSE_HIDDEN, vb.pp("fuse1"))?,
            fuse2: linear(FUSE_HIDDEN, HEAD_DIM, vb.pp("fuse2"))?,
            head: linear(HEAD_DIM, num_classes, vb.pp("head"))?,
        })
    }

    /// `text_emb`: [batch, 384] from MiniLmEncoder
    /// `params`:   [batch, 64]  numeric feature vector
    pub fn forward(&self, text_emb: &Tensor, params: &Tensor) -> Result<Tensor> {
        // Numeric branch.
        let n = self.num_fc1.forward(params)?.relu()?;
        let n = self.num_fc2.forward(&n)?.relu()?; // [batch, 64]

        // Fuse text + numeric.
        let combined = Tensor::cat(&[text_emb, &n], 1)?; // [batch, 448]
        let f = self.fuse1.forward(&combined)?.relu()?; // [batch, 128]
        let f = self.fuse2.forward(&f)?.relu()?; // [batch, 32]

        // L2 normalise before the final linear.
        let norm = f.sqr()?.sum_keepdim(1)?.sqrt()?;
        let f = f.broadcast_div(&norm)?;

        self.head.forward(&f)
    }
}
