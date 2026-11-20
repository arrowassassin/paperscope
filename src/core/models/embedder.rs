use super::specs::ModelSpec;
use anyhow::{bail, Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use std::path::Path;
use tokenizers::Tokenizer;

/// A local sentence embedder running on Hugging Face Candle (pure Rust, no
/// ONNX Runtime, no Python, no network at inference time).
pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
    spec: ModelSpec,
}

impl Embedder {
    /// Load from a verified model directory (`config.json`, `tokenizer.json`,
    /// `model.safetensors`).
    pub fn new(model_dir: &Path, spec: &ModelSpec) -> Result<Self> {
        let config_path = model_dir.join("config.json");
        let config_text = std::fs::read_to_string(&config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let raw: serde_json::Value = serde_json::from_str(&config_text)?;
        let model_type = raw.get("model_type").and_then(|v| v.as_str()).unwrap_or("");
        if model_type != "bert" {
            // Fail loudly rather than returning plausible-looking garbage vectors.
            bail!(
                "unsupported encoder architecture {model_type:?} for model {}",
                spec.id
            );
        }
        let config: BertConfig = serde_json::from_value(raw).context("parsing BERT config")?;
        if config.hidden_size != spec.dims {
            bail!(
                "config hidden_size {} does not match spec dims {} for {}",
                config.hidden_size,
                spec.dims,
                spec.id
            );
        }

        let device = Device::Cpu;
        let tokenizer_path = model_dir.join("tokenizer.json");
        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("loading {}: {e}", tokenizer_path.display()))?;
        tokenizer
            .with_padding(None)
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length: spec.max_tokens,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!("configuring truncation: {e}"))?;

        let weights = model_dir.join("model.safetensors");
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[&weights], DType::F32, &device)? };
        let model = BertModel::load(vb, &config).context("loading encoder weights")?;

        Ok(Self {
            model,
            tokenizer,
            device,
            spec: spec.clone(),
        })
    }

    pub fn dims(&self) -> usize {
        self.spec.dims
    }

    pub fn model_id(&self) -> &str {
        self.spec.id
    }

    /// Embed a search query, applying the model's retrieval instruction.
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_with_prefix(self.spec.query_prefix, text)
    }

    /// Embed a document chunk.
    pub fn embed_document(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_with_prefix(self.spec.document_prefix, text)
    }

    /// Backwards-compatible alias for document embeddings.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_document(text)
    }

    fn embed_with_prefix(&self, prefix: &str, text: &str) -> Result<Vec<f32>> {
        let input = if prefix.is_empty() {
            text.to_string()
        } else {
            format!("{prefix}{text}")
        };
        let encoding = self
            .tokenizer
            .encode(input, true)
            .map_err(|e| anyhow::anyhow!("tokenizing input: {e}"))?;

        let input_ids = Tensor::new(encoding.get_ids(), &self.device)?.unsqueeze(0)?;
        let token_type_ids = Tensor::new(encoding.get_type_ids(), &self.device)?.unsqueeze(0)?;
        let attention_mask =
            Tensor::new(encoding.get_attention_mask(), &self.device)?.unsqueeze(0)?;

        let hidden = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;

        // Mean pooling over real tokens only (never over padding).
        let mask = attention_mask.to_dtype(DType::F32)?.unsqueeze(2)?;
        let summed = hidden.broadcast_mul(&mask)?.sum(1)?;
        let counts = mask.sum(1)?.clamp(1e-9, f64::INFINITY)?;
        let pooled = summed.broadcast_div(&counts)?;

        // L2-normalised, so sqlite-vec's cosine distance is `1 - dot`.
        let norm = pooled
            .sqr()?
            .sum_keepdim(1)?
            .sqrt()?
            .clamp(1e-12, f64::INFINITY)?;
        let normalized = pooled.broadcast_div(&norm)?;
        Ok(normalized.flatten_all()?.to_vec1::<f32>()?)
    }
}
