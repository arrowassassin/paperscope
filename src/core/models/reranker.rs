use crate::core::search::Hit;
use anyhow::{Context, Result};
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::xlm_roberta::{
    Config as XlmConfig, XLMRobertaForSequenceClassification,
};
use std::path::Path;
use tokenizers::Tokenizer;

/// Cross-encoder reranker (`BAAI/bge-reranker-v2-m3`, an XLM-RoBERTa sequence
/// classifier with a single logit). Runs entirely on the local CPU via Candle.
pub struct Reranker {
    model: XLMRobertaForSequenceClassification,
    tokenizer: Tokenizer,
    device: Device,
    max_tokens: usize,
}

impl Reranker {
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let device = Device::Cpu;
        let dir = model_path.parent().context("model path has no parent")?;
        let config_text = std::fs::read_to_string(dir.join("config.json"))
            .context("reading reranker config.json")?;
        let config: XlmConfig = serde_json::from_str(&config_text)?;
        let max_tokens = config.max_position_embeddings.min(512);

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow::anyhow!("loading {}: {e}", tokenizer_path.display()))?;
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[model_path], DType::F32, &device)? };
        let model = XLMRobertaForSequenceClassification::new(1, &config, vb)?;

        Ok(Self {
            model,
            tokenizer,
            device,
            max_tokens,
        })
    }

    /// Score every `(query, passage)` pair and return hits sorted best-first,
    /// with `score` replaced by the sigmoid of the cross-encoder logit.
    pub fn rerank(&self, query: &str, hits: Vec<Hit>) -> Result<Vec<Hit>> {
        if hits.is_empty() {
            return Ok(hits);
        }
        let mut tokenizer = self.tokenizer.clone();
        tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                max_length: self.max_tokens,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!("configuring truncation: {e}"))?;

        let mut scored: Vec<(Hit, f32)> = Vec::with_capacity(hits.len());
        for hit in hits {
            let encoding = tokenizer
                .encode((query.to_string(), hit.snippet.clone()), true)
                .map_err(|e| anyhow::anyhow!("tokenizing pair: {e}"))?;
            let input_ids =
                Tensor::new(encoding.get_ids(), &self.device)?.unsqueeze(0)?;
            let attention_mask =
                Tensor::new(encoding.get_attention_mask(), &self.device)?.unsqueeze(0)?;
            let token_type_ids = input_ids.zeros_like()?;

            let logits = self
                .model
                .forward(&input_ids, &attention_mask, &token_type_ids)?;
            let logit = logits.i((0, 0))?.to_scalar::<f32>()?;
            let score = 1.0 / (1.0 + (-logit).exp());
            let mut hit = hit;
            hit.score = score;
            scored.push((hit, score));
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().map(|(h, _)| h).collect())
    }
}
