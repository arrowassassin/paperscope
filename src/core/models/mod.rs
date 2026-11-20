pub mod embedder;
pub mod manager;
pub mod reranker;
pub mod specs;

pub use embedder::Embedder;
pub use manager::{verify_sha256 as verify_sha256_public, ModelInfo, ModelManager};
pub use reranker::Reranker;
pub use specs::{
    all_specs, spec, ModelKind, ModelSpec, DEFAULT_EMBEDDING, EMBEDDING_DIMS, MULTILINGUAL_EMBEDDING,
    OPTIONAL_RERANKER,
};
