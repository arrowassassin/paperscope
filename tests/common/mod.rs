//! Shared test helpers: a deterministic, offline embedder.
//!
//! Tests must never download weights, so semantic behaviour is exercised with a
//! hashing bag-of-words embedder: documents sharing words land near each other.

use anyhow::Result;
use paper::core::index::EMBEDDING_DIMS;
use paper::core::search::QueryEmbedder;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub struct HashEmbedder;

impl QueryEmbedder for HashEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut v = vec![0.0f32; EMBEDDING_DIMS];
        for token in text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() > 2)
        {
            let mut h = DefaultHasher::new();
            token.hash(&mut h);
            let idx = (h.finish() as usize) % EMBEDDING_DIMS;
            v[idx] += 1.0;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        Ok(v)
    }
}

pub fn hash_embedder() -> paper::core::search::SharedEmbedder {
    std::sync::Arc::new(HashEmbedder)
}
