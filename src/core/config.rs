use super::paths::{create_private_dir, Paths};
use serde::{Deserialize, Serialize};
use std::io::Write;
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
    /// Optional local, OpenAI-compatible server (Ollama, LM Studio, vLLM).
    /// Only used by `paper answer --polish --engine local`.
    pub local_engine: LocalEngineConfig,
    /// Optional cloud fallback. Only used by `paper answer --polish
    /// --engine openrouter`, and only when an API key is configured.
    pub openrouter: OpenRouterConfig,
}

/// Local OpenAI-compatible server used for optional answer polishing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalEngineConfig {
    /// Base URL of the server, e.g. `http://localhost:11434`.
    pub base_url: String,
    /// Model name as the server knows it, e.g. `llama3.1`.
    pub model: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for LocalEngineConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            model: String::new(),
            timeout_secs: 60,
        }
    }
}

/// OpenRouter settings for the opt-in cloud fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenRouterConfig {
    /// API key. When empty, `OPENROUTER_API_KEY` from the environment is used.
    pub api_key: String,
    /// Model slug, e.g. `anthropic/claude-sonnet-4-5`.
    pub model: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: String::new(),
            timeout_secs: 120,
        }
    }
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
            local_engine: LocalEngineConfig::default(),
            openrouter: OpenRouterConfig::default(),
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
            match toml::from_str(&content) {
                Ok(config) => Ok(config),
                Err(e) => {
                    tracing::warn!(
                        "Config file at {} is corrupt ({e:#}); falling back to defaults",
                        path.display()
                    );
                    Ok(Self::default())
                }
            }
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&Paths::new().config_path())
    }

    /// Owner-only on Unix: the file may hold the OpenRouter API key. Written to
    /// a private temp file in the same directory and renamed over `path`, so a
    /// reader (or a crash) never sees a half-written config, and the new file
    /// is 0600 before any content lands in it (tempfile creates it `create_new`
    /// with an unpredictable name; mode 0600 on Unix). A symlinked config is
    /// written through, as before.
    pub fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let parent = path.parent().unwrap_or(std::path::Path::new("."));
        create_private_dir(parent)?;
        let contents = toml::to_string_pretty(self)?;
        let mut temp = tempfile::Builder::new()
            .prefix(".config-")
            .suffix(".tmp")
            .tempfile_in(parent)?;
        temp.write_all(contents.as_bytes())?;
        temp.as_file().sync_all()?;
        // Replaces an existing file (std's rename semantics; MoveFileEx on Windows).
        // On Windows a replace can fail with "access denied" while another
        // writer's replace of the same file is still completing, or while a
        // scanner briefly holds it; that clears in milliseconds, so retry.
        let mut attempt = 0;
        loop {
            match temp.persist(&path) {
                Ok(_) => return Ok(()),
                Err(e)
                    if cfg!(windows)
                        && e.error.kind() == std::io::ErrorKind::PermissionDenied
                        && attempt < 50 =>
                {
                    temp = e.file;
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => return Err(e.error.into()),
            }
        }
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
