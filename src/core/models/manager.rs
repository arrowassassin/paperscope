use super::specs::{all_specs, ModelKind, ModelSpec, DEFAULT_EMBEDDING, OPTIONAL_RERANKER};
use super::{Embedder, Reranker};
use crate::core::paths::Paths;
use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Lazily downloads, verifies and loads the two supported models.
pub struct ModelManager {
    cache_dir: PathBuf,
    client: Client,
    specs: HashMap<&'static str, ModelSpec>,
    /// When true, no network access happens; missing models are an error.
    offline: bool,
}

impl Default for ModelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelManager {
    pub fn new() -> Self {
        Self::with_cache_dir(Paths::new().model_cache_dir())
    }

    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        let offline = std::env::var("PAPER_OFFLINE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            cache_dir,
            client: Client::new(),
            specs: all_specs(),
            offline,
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn model_path(&self, model_id: &str) -> PathBuf {
        self.cache_dir.join(format!("{model_id}.safetensors"))
    }

    fn tokenizer_path(&self, model_id: &str) -> PathBuf {
        self.cache_dir.join(format!("{model_id}_tokenizer.json"))
    }

    pub fn spec(&self, model_id: &str) -> Result<&ModelSpec> {
        self.specs
            .get(model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown model: {model_id}"))
    }

    /// Return a verified local copy of the model, downloading it if necessary.
    pub async fn ensure(&self, model_id: &str) -> Result<PathBuf> {
        let spec = self.spec(model_id)?.clone();
        tokio::fs::create_dir_all(&self.cache_dir).await?;
        let dest = self.model_path(model_id);

        if dest.exists() && Self::verify_sha256(&dest, spec.sha256)? {
            self.ensure_tokenizer(model_id, spec.tokenizer_url).await?;
            return Ok(dest);
        }
        if dest.exists() {
            tracing::warn!("cached {model_id} failed checksum verification; re-downloading");
        }
        if self.offline {
            bail!("model {model_id} is not installed and PAPER_OFFLINE is set");
        }

        self.download(spec.url, &dest, spec.name, spec.size_mb).await?;
        if !Self::verify_sha256(&dest, spec.sha256)? {
            let _ = tokio::fs::remove_file(&dest).await;
            bail!("checksum mismatch for {model_id}; deleted the corrupt download");
        }
        self.ensure_tokenizer(model_id, spec.tokenizer_url).await?;
        Ok(dest)
    }

    async fn ensure_tokenizer(&self, model_id: &str, url: &str) -> Result<PathBuf> {
        let path = self.tokenizer_path(model_id);
        if path.exists() {
            return Ok(path);
        }
        if self.offline {
            bail!("tokenizer for {model_id} is missing and PAPER_OFFLINE is set");
        }
        self.download(url, &path, "tokenizer", 0).await?;
        Ok(path)
    }

    async fn download(&self, url: &str, dest: &Path, label: &str, size_mb: u64) -> Result<()> {
        let pb = ProgressBar::new(size_mb * 1_000_000);
        pb.set_style(
            ProgressStyle::with_template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("##-"),
        );
        pb.set_message(format!("Downloading {label}"));

        let mut resp = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("downloading {url}"))?
            .error_for_status()
            .with_context(|| format!("download failed for {url}"))?;
        if let Some(total) = resp.content_length() {
            pb.set_length(total);
        }

        // Write to a temporary file first so an interrupted download can never be
        // mistaken for a valid model.
        let tmp = dest.with_extension("partial");
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut downloaded = 0u64;
        while let Some(chunk) = resp.chunk().await? {
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }
        file.flush().await?;
        drop(file);
        tokio::fs::rename(&tmp, dest).await?;
        pb.finish_and_clear();
        Ok(())
    }

    pub fn verify_sha256(path: &Path, expected: &str) -> Result<bool> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        let actual = hex::encode(hasher.finalize());
        Ok(actual.eq_ignore_ascii_case(expected))
    }

    pub fn is_installed(&self, model_id: &str) -> bool {
        let Ok(spec) = self.spec(model_id) else {
            return false;
        };
        self.model_path(model_id).exists()
            && Self::verify_sha256(&self.model_path(model_id), spec.sha256).unwrap_or(false)
    }

    pub fn list(&self) -> Vec<ModelInfo> {
        let mut infos: Vec<ModelInfo> = self
            .specs
            .values()
            .map(|s| ModelInfo {
                id: s.id,
                name: s.name,
                size_mb: s.size_mb,
                kind: s.kind,
                license: s.license,
                installed: self.model_path(s.id).exists(),
            })
            .collect();
        infos.sort_by_key(|m| m.id);
        infos
    }

    pub async fn get_embedder(&self) -> Result<Embedder> {
        let path = self.ensure(DEFAULT_EMBEDDING).await?;
        Embedder::new(&path, &self.tokenizer_path(DEFAULT_EMBEDDING))
    }

    pub async fn get_reranker(&self) -> Result<Reranker> {
        let path = self.ensure(OPTIONAL_RERANKER).await?;
        Reranker::new(&path, &self.tokenizer_path(OPTIONAL_RERANKER))
    }

    /// The reranker is opt-in: only load it when it is already present locally.
    pub async fn reranker_if_installed(&self) -> Result<Option<Reranker>> {
        if self.model_path(OPTIONAL_RERANKER).exists() {
            Ok(Some(self.get_reranker().await?))
        } else {
            Ok(None)
        }
    }

    pub async fn remove(&self, model_id: &str) -> Result<()> {
        let _ = tokio::fs::remove_file(self.model_path(model_id)).await;
        let _ = tokio::fs::remove_file(self.tokenizer_path(model_id)).await;
        Ok(())
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub size_mb: u64,
    pub kind: ModelKind,
    pub license: &'static str,
    pub installed: bool,
}
