pub mod manager;
pub mod specs;

#[cfg(feature = "embeddings")]
pub mod embedder;
#[cfg(feature = "embeddings")]
pub mod reranker;

/// Keyword-only builds keep the same API without pulling in Candle.
#[cfg(not(feature = "embeddings"))]
mod without_embeddings;

pub use manager::{verify_sha256 as verify_sha256_public, ModelInfo, ModelManager};
pub use specs::{
    all_specs, spec, ModelKind, ModelSpec, DEFAULT_EMBEDDING, EMBEDDING_DIMS,
    MULTILINGUAL_EMBEDDING, OPTIONAL_RERANKER,
};

#[cfg(feature = "embeddings")]
pub use embedder::Embedder;
#[cfg(feature = "embeddings")]
pub use reranker::Reranker;

#[cfg(not(feature = "embeddings"))]
pub use without_embeddings::{Embedder, Reranker};
