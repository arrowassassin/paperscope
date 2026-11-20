use std::path::{Path, PathBuf};

/// Resolves every on-disk location Paper uses.
///
/// The base directory is taken from `PAPER_HOME` when set (used by tests and by
/// users who want a portable install), otherwise `dirs::cache_dir()/paper`.
pub struct Paths {
    base: PathBuf,
    config_base: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}

impl Paths {
    pub fn new() -> Self {
        if let Some(home) = std::env::var_os("PAPER_HOME") {
            let base = PathBuf::from(home);
            return Self {
                base: base.clone(),
                config_base: base,
            };
        }

        let base = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("paper");
        let config_base = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("paper");
        Self { base, config_base }
    }

    /// Paths rooted at an explicit directory. Useful for tests.
    pub fn with_base(base: impl AsRef<Path>) -> Self {
        let base = base.as_ref().to_path_buf();
        Self {
            config_base: base.clone(),
            base,
        }
    }

    pub fn base_dir(&self) -> PathBuf {
        self.base.clone()
    }

    pub fn model_cache_dir(&self) -> PathBuf {
        self.base.join("models")
    }

    pub fn db_path(&self) -> PathBuf {
        self.base.join("chunks.db")
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_base.join("config.toml")
    }

    /// Create the directories Paper writes into. Safe to call repeatedly.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.base)?;
        std::fs::create_dir_all(self.model_cache_dir())?;
        if let Some(parent) = self.config_path().parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}
