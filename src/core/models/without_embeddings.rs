//! Stubs for builds made without the `embeddings` feature.
//!
//! `cargo build --no-default-features` produces a keyword-only binary: no
//! Candle, no tokenizers, no inference stack. The types keep the same API so
//! the CLI, the MCP server and the search pipeline compile unchanged, and
//! every attempt to load a model fails with an actionable message instead of a
//! missing symbol.

use super::specs::{LanguageCoverage, ModelSpec};
use crate::core::search::Hit;
use anyhow::{bail, Result};
use std::path::Path;

const MESSAGE: &str = "this build has no embedding support (rebuild with --features embeddings)";

/// Placeholder for the Candle-backed embedder.
pub struct Embedder;

impl Embedder {
    pub fn new(_model_dir: &Path, _spec: &ModelSpec) -> Result<Self> {
        bail!(MESSAGE)
    }

    pub fn dims(&self) -> usize {
        0
    }

    pub fn model_id(&self) -> &str {
        ""
    }

    pub fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
        bail!(MESSAGE)
    }

    pub fn embed_document(&self, _text: &str) -> Result<Vec<f32>> {
        bail!(MESSAGE)
    }

    pub fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        bail!(MESSAGE)
    }
}

/// Placeholder for the Candle-backed cross-encoder reranker.
pub struct Reranker;

impl Reranker {
    pub fn new(_model_dir: &Path, _spec: &ModelSpec) -> Result<Self> {
        bail!(MESSAGE)
    }

    pub fn coverage(&self) -> LanguageCoverage {
        LanguageCoverage::Multilingual // unreachable: `new` always fails
    }

    pub fn rerank(&self, _query: &str, _hits: Vec<Hit>) -> Result<Vec<Hit>> {
        bail!(MESSAGE)
    }
}
