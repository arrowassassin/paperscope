use super::specs::{all_specs, ModelKind, ModelSpec, DEFAULT_EMBEDDING, OPTIONAL_RERANKER};
use super::{Embedder, Reranker};
use crate::core::paths::Paths;
use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::io::AsyncWriteExt;

/// Downloads, verifies and loads models. Every artifact is pinned by commit and
/// SHA256; nothing is fetched implicitly during search.
pub struct ModelManager {
    cache_dir: PathBuf,
    client: Client,
    specs: Vec<ModelSpec>,
    offline: bool,
    /// Installed model IDs cached in memory so repeated `is_installed()` calls
    /// don't re-hash multi-GB model files on every search (I1).
    installed_cache: Mutex<HashSet<String>>,
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
            installed_cache: Mutex::new(HashSet::new()),
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn specs(&self) -> &[ModelSpec] {
        &self.specs
    }

    pub fn spec(&self, model_id: &str) -> Result<&ModelSpec> {
        self.specs
            .iter()
            .find(|s| s.id == model_id)
            .ok_or_else(|| anyhow::anyhow!("unknown model: {model_id}"))
    }

    pub fn model_dir(&self, model_id: &str) -> PathBuf {
        self.cache_dir.join(model_id)
    }

    pub fn file_path(&self, model_id: &str, file: &str) -> PathBuf {
        self.model_dir(model_id).join(file)
    }

    /// True when every artifact is present *and* matches its pinned hash.
    /// Results are cached so repeated calls don't re-hash multi-GB files (I1).
    pub fn is_installed(&self, model_id: &str) -> bool {
        // Fast path: already verified in this process.
        {
            let cache = self.installed_cache.lock().unwrap();
            if cache.contains(model_id) {
                return true;
            }
        }
        let Ok(spec) = self.spec(model_id) else {
            return false;
        };
        let installed = spec.files.iter().all(|f| {
            let path = self.file_path(model_id, f.name);
            path.exists() && verify_sha256(&path, f.sha256).unwrap_or(false)
        });
        if installed {
            self.installed_cache
                .lock()
                .unwrap()
                .insert(model_id.to_string());
        }
        installed
    }

    /// Ensure every file of `model_id` is present and verified, downloading what
    /// is missing or corrupt. Returns the model directory.
    pub async fn ensure(&self, model_id: &str) -> Result<PathBuf> {
        let spec = self.spec(model_id)?.clone();
        let dir = self.model_dir(model_id);
        tokio::fs::create_dir_all(&dir).await?;

        for file in spec.files {
            let dest = dir.join(file.name);
            if dest.exists() && verify_sha256(&dest, file.sha256)? {
                continue;
            }
            if dest.exists() {
                tracing::warn!("{} failed checksum verification; re-downloading", file.name);
            }
            if self.offline {
                bail!(
                    "{model_id}/{} is missing or corrupt and PAPER_OFFLINE is set",
                    file.name
                );
            }
            let url = format!(
                "https://huggingface.co/{}/resolve/{}/{}",
                spec.repo, spec.revision, file.name
            );
            self.download(&url, &dest, &format!("{model_id}/{}", file.name), file.size)
                .await?;
            if !verify_sha256(&dest, file.sha256)? {
                let _ = tokio::fs::remove_file(&dest).await;
                bail!(
                    "checksum mismatch for {model_id}/{}; deleted the corrupt download",
                    file.name
                );
            }
        }
        self.installed_cache
            .lock()
            .unwrap()
            .insert(model_id.to_string());
        Ok(dir)
    }

    async fn download(&self, url: &str, dest: &Path, label: &str, size: u64) -> Result<()> {
        // Resume support: if a `.partial` file exists, request bytes from where
        // we left off (I2). Hugging Face supports Range requests.
        let tmp = dest.with_extension("partial");
        let mut downloaded = if tmp.exists() {
            tmp.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        let pb = ProgressBar::new(size);
        pb.set_style(
            ProgressStyle::with_template(
                "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
            )
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("##-"),
        );
        pb.set_message(format!("Downloading {label}"));
        pb.set_position(downloaded);

        let mut builder = self.client.get(url);
        if downloaded > 0 {
            builder = builder.header(reqwest::header::RANGE, format!("bytes={downloaded}-"));
        }
        let mut resp = builder
            .send()
            .await
            .with_context(|| format!("downloading {url}"))?
            .error_for_status()
            .with_context(|| format!("download failed for {url}"))?;

        // A resumed request must be answered with 206 Partial Content. If the
        // server ignored the Range header and sent the whole file (200), the
        // existing `.partial` prefix is stale: start it over instead of
        // appending a second copy and failing the checksum later.
        if downloaded > 0 && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            tracing::debug!("server ignored Range for {label}; restarting download");
            tokio::fs::remove_file(&tmp).await?;
            downloaded = 0;
            pb.set_position(0);
        }

        // `.partial` first: an interrupted download is never mistaken for a model.
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&tmp)
            .await?;
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

    pub fn list(&self) -> Vec<ModelInfo> {
        self.specs
            .iter()
            .map(|s| ModelInfo {
                id: s.id.to_string(),
                name: s.name.to_string(),
                size_mb: s.size_mb(),
                kind: s.kind,
                license: s.license.to_string(),
                languages: s.languages.to_string(),
                installed: self.is_installed(s.id),
                is_default: s.id == DEFAULT_EMBEDDING,
            })
            .collect()
    }

    pub fn load_embedder(&self, model_id: &str) -> Result<Embedder> {
        let spec = self.spec(model_id)?;
        if spec.kind != ModelKind::Embedding {
            bail!("{model_id} is not an embedding model");
        }
        Embedder::new(&self.model_dir(model_id), spec)
    }

    /// Load the default embedder if (and only if) it is already on disk.
    pub fn embedder_if_installed(&self, model_id: &str) -> Option<Embedder> {
        match self.load_embedder(model_id) {
            Ok(e) => Some(e),
            Err(e) => {
                tracing::debug!("embedder {model_id} unavailable: {e}");
                None
            }
        }
    }

    pub fn load_reranker(&self) -> Result<Reranker> {
        let spec = self.spec(OPTIONAL_RERANKER)?;
        Reranker::new(&self.model_dir(OPTIONAL_RERANKER), spec)
    }

    pub fn reranker_if_installed(&self) -> Option<Reranker> {
        match self.load_reranker() {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::debug!("reranker unavailable: {e}");
                None
            }
        }
    }

    pub async fn remove(&self, model_id: &str) -> Result<()> {
        self.spec(model_id)?;
        let dir = self.model_dir(model_id);
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }
}

pub fn verify_sha256(path: &Path, expected: &str) -> Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let actual = hex::encode(hasher.finalize());
    Ok(actual.eq_ignore_ascii_case(expected))
}

#[derive(Debug, serde::Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub size_mb: u64,
    pub kind: ModelKind,
    pub license: String,
    pub languages: String,
    pub installed: bool,
    pub is_default: bool,
}
