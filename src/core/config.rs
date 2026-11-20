use super::paths::Paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub keyword_sufficiency_threshold: usize,
    pub rerank_top: usize,
    pub rrf_k: i32,
    pub max_chunk_tokens: usize,
    pub chunk_overlap: usize,
    pub roots: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keyword_sufficiency_threshold: 10,
            rerank_top: 20,
            rrf_k: 60,
            max_chunk_tokens: 400,
            chunk_overlap: 40,
            roots: vec![],
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from(&Paths::new().config_path())
    }

    pub fn load_from(path: &std::path::Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Paths::new().config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn search(&self) -> SearchConfig {
        SearchConfig {
            keyword_sufficiency_threshold: self.keyword_sufficiency_threshold,
            rerank_top: self.rerank_top,
            rrf_k: self.rrf_k,
            ..SearchConfig::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub keyword_sufficiency_threshold: usize,
    pub rerank_top: usize,
    pub rrf_k: i32,
    pub keyword_limit: usize,
    pub semantic_limit: usize,
    pub fused_limit: usize,
    /// Minimum cosine similarity for a semantic hit. Vector search always has a
    /// nearest neighbour, so without a floor every query would "find" something.
    pub semantic_min_score: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            keyword_sufficiency_threshold: 10,
            rerank_top: 20,
            rrf_k: 60,
            keyword_limit: 20,
            semantic_limit: 20,
            fused_limit: 50,
            semantic_min_score: 0.25,
        }
    }
}
