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

    /// Owner-only on Unix: the file may hold the OpenRouter API key.
    pub fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            create_private_dir(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
            // Before writing, so new contents never land in a readable file.
            restrict_existing(path);
        }
        options.open(path)?.write_all(contents.as_bytes())?;
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

/// Best-effort: drops group/other bits from an existing config file. A failure
/// is logged rather than returned so a save that used to succeed still does.
#[cfg(unix)]
fn restrict_existing(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    // symlink_metadata: never chmod through a symlink to a file Paper does not own.
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    let mode = metadata.permissions().mode();
    if !metadata.file_type().is_file() || mode & 0o077 == 0 {
        return;
    }
    let restricted = std::fs::Permissions::from_mode(mode & !0o077);
    if let Err(e) = std::fs::set_permissions(path, restricted) {
        tracing::warn!("could not restrict permissions on {}: {e}", path.display());
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
