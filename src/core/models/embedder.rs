use anyhow::{bail, Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use std::path::Path;
use tokenizers::Tokenizer;

/// Sentence embedder backed by a Hugging Face encoder, run locally with Candle.
///
/// The encoder is selected from `config.json` next to the weights so that an
/// unsupported architecture fails loudly instead of producing garbage vectors.
pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
    dims: usize,
    max_tokens: usize,
}

impl Embedder {
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let dir = model_path
            .parent()
            .context("model path has no parent directory")?;
        let config = read_config(dir)?;
        if config.model_type != "bert" {
            bail!(
                "unsupported encoder architecture {:?} in {}; only `bert` is implemented so far",
                config.model_type,
                dir.join("config.json").display()
            );
        }

        let device = Device::Cpu;
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("loading {}: {e}", tokenizer_path.display()))?;
        let bert_config: BertConfig = serde_json::from_value(config.raw)
            .context("parsing BERT config.json")?;
        let dims = bert_config.hidden_size;
        let max_tokens = bert_config.max_position_embeddings.min(512);

        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[model_path], DType::F32, &device)? };
        let model = BertModel::load(vb, &bert_config).context("loading encoder weights")?;

        Ok(Self {
            model,
            tokenizer,
            device,
            dims,
            max_tokens,
        })
    }

    pub fn dims(&self) -> usize {
        self.dims
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut tokenizer = self.tokenizer.clone();
        tokenizer.with_truncation(Some(tokenizers::TruncationParams {
            max_length: self.max_tokens,
            ..Default::default()
        })).map_err(|e| anyhow::anyhow!("configuring truncation: {e}"))?;

        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("tokenizing input: {e}"))?;
        let ids = encoding.get_ids().to_vec();
        let type_ids = encoding.get_type_ids().to_vec();
        let mask = encoding.get_attention_mask().to_vec();

        let input_ids = Tensor::new(ids.as_slice(), &self.device)?.unsqueeze(0)?;
        let token_type_ids = Tensor::new(type_ids.as_slice(), &self.device)?.unsqueeze(0)?;
        let attention_mask = Tensor::new(mask.as_slice(), &self.device)?.unsqueeze(0)?;

        let hidden = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;

        // Mean pooling, ignoring padding.
        let mask = attention_mask.to_dtype(DType::F32)?.unsqueeze(2)?;
        let summed = hidden.broadcast_mul(&mask)?.sum(1)?;
        let counts = mask.sum(1)?.clamp(1e-9, f64::INFINITY)?;
        let pooled = summed.broadcast_div(&counts)?;

        // L2 normalise so cosine similarity is a dot product.
        let norm = pooled.sqr()?.sum_keepdim(1)?.sqrt()?.clamp(1e-12, f64::INFINITY)?;
        let normalized = pooled.broadcast_div(&norm)?;
        Ok(normalized.flatten_all()?.to_vec1::<f32>()?)
    }
}

struct RawConfig {
    model_type: String,
    raw: serde_json::Value,
}

fn read_config(dir: &Path) -> Result<RawConfig> {
    let path = dir.join("config.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}; is the model fully downloaded?", path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&text)?;
    let model_type = raw
        .get("model_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    Ok(RawConfig { model_type, raw })
}
