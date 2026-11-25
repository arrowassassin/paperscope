use super::specs::{LanguageCoverage, ModelSpec};
use crate::core::search::Hit;
use anyhow::{Context, Result};
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::xlm_roberta::{
    Config as XlmConfig, XLMRobertaForSequenceClassification,
};
use std::path::Path;
use tokenizers::Tokenizer;

/// An XLM-RoBERTa sequence classifier producing one
/// relevance logit per `(query, passage)` pair. Runs on the local CPU.
pub struct Reranker {
    model: XLMRobertaForSequenceClassification,
    tokenizer: Tokenizer,
    device: Device,
    coverage: LanguageCoverage,
}

impl Reranker {
    pub fn new(model_dir: &Path, spec: &ModelSpec) -> Result<Self> {
        let device = Device::Cpu;
        let config_path = model_dir.join("config.json");
        let config_text = std::fs::read_to_string(&config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let config: XlmConfig = serde_json::from_str(&config_text)?;

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
        let model = XLMRobertaForSequenceClassification::new(1, &config, vb)?;

        Ok(Self {
            model,
            tokenizer,
            device,
            coverage: spec.coverage,
        })
    }

    pub fn coverage(&self) -> LanguageCoverage {
        self.coverage
    }

    /// Re-score hits with the cross-encoder, best first. `Hit::score` becomes a
    /// calibrated-ish probability in `0..1` (sigmoid of the relevance logit).
    pub fn rerank(&self, query: &str, hits: Vec<Hit>) -> Result<Vec<Hit>> {
        if hits.is_empty() {
            return Ok(hits);
        }
        let mut scored: Vec<(Hit, f32)> = Vec::with_capacity(hits.len());
        for hit in hits {
            let encoding = self
                .tokenizer
                .encode((query.to_string(), hit.content.clone()), true)
                .map_err(|e| anyhow::anyhow!("tokenizing pair: {e}"))?;
            let input_ids = Tensor::new(encoding.get_ids(), &self.device)?.unsqueeze(0)?;
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
