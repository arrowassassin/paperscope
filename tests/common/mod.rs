//! Shared test helpers: a deterministic, offline embedder.
//!
//! Tests must never download weights, so semantic behaviour is exercised with a
//! hashing bag-of-words embedder: documents sharing words land near each other.

// Each test binary uses only part of this module.
#![allow(dead_code)]

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

    fn model_id(&self) -> &str {
        "test-hash-embedder"
    }
}

pub fn hash_embedder() -> paper::core::search::SharedEmbedder {
    std::sync::Arc::new(HashEmbedder)
}

/// Sign `body` with a fresh Ed25519 key, save the pair into `home` as the
/// cached remote config, and return the public key (hex) the binary must trust
/// via `PAPER_REMOTE_CONFIG_TEST_PUBKEY` (debug builds only).
pub fn install_signed_config(home: &std::path::Path, body: &str) -> String {
    use base64::Engine;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
    let kp = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let sig = base64::engine::general_purpose::STANDARD.encode(kp.sign(body.as_bytes()).as_ref());
    paper::core::remote_config::save_verified(home, body.as_bytes(), &sig).unwrap();
    hex::encode(kp.public_key().as_ref())
}
