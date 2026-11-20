use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelKind {
    Embedding,
    Reranker,
}

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ModelKind,
    pub size_mb: u64,
    /// Pinned download location. Never `main` for release builds.
    pub url: &'static str,
    pub tokenizer_url: &'static str,
    /// SHA256 of the safetensors payload. Verified before the model is used.
    pub sha256: &'static str,
    pub license: &'static str,
    pub dims: usize,
    pub context: usize,
}

pub const DEFAULT_EMBEDDING: &str = "embeddinggemma";
pub const OPTIONAL_RERANKER: &str = "bge-reranker-v2-m3";

pub fn all_specs() -> HashMap<&'static str, ModelSpec> {
    let mut m = HashMap::new();
    m.insert(
        DEFAULT_EMBEDDING,
        ModelSpec {
            id: DEFAULT_EMBEDDING,
            name: "EmbeddingGemma (default, multilingual, 768 dims)",
            kind: ModelKind::Embedding,
            size_mb: 200,
            url: "https://huggingface.co/google/embeddinggemma/resolve/main/model.safetensors",
            tokenizer_url:
                "https://huggingface.co/google/embeddinggemma/resolve/main/tokenizer.json",
            // TODO(release): replace with the real digest of the pinned artifact.
            sha256: "UNPINNED",
            license: "Gemma Terms of Use",
            dims: 768,
            context: 2048,
        },
    );
    m.insert(
        OPTIONAL_RERANKER,
        ModelSpec {
            id: OPTIONAL_RERANKER,
            name: "BGE Reranker v2-m3 (precision, opt-in)",
            kind: ModelKind::Reranker,
            size_mb: 1100,
            url: "https://huggingface.co/BAAI/bge-reranker-v2-m3/resolve/main/model.safetensors",
            tokenizer_url:
                "https://huggingface.co/BAAI/bge-reranker-v2-m3/resolve/main/tokenizer.json",
            // TODO(release): replace with the real digest of the pinned artifact.
            sha256: "UNPINNED",
            license: "Apache-2.0",
            dims: 0,
            context: 8192,
        },
    );
    m
}
