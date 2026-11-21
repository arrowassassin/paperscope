use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Downloads a single model file with progress, resumable support, SHA256 verification,
/// and atomic rename.
pub struct ModelDownloader {
    url: String,
    dest: PathBuf,
    expected_sha256: String,
    total_size: u64,
    client: Client,
}

impl ModelDownloader {
    /// Create a new ModelDownloader.
    pub fn new(url: String, dest: PathBuf, expected_sha256: String, total_size: u64) -> Self {
        Self {
            url,
            dest,
            expected_sha256,
            total_size,
            client: Client::new(),
        }
    }

    /// Download the file with progress bar, resume support, SHA256 verification,
    /// and atomic rename. Respects PAPER_OFFLINE=1.
    pub async fn download_with_progress(&self) -> Result<()> {
        // Check PAPER_OFFLINE
        let offline = std::env::var("PAPER_OFFLINE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let tmp = self.dest.with_extension("partial");
        let mut downloaded = if tmp.exists() {
            tmp.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        // If offline and file not complete, error
        if offline {
            if downloaded >= self.total_size && self.verify_sha256(&tmp).await? {
                tokio::fs::rename(&tmp, &self.dest).await?;
                return Ok(());
            }
            bail!(
                "{} is missing or incomplete and PAPER_OFFLINE is set",
                self.dest.display()
            );
        }

        let pb = ProgressBar::new(self.total_size);
        pb.set_style(
            ProgressStyle::with_template(
                "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
            )
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("##-"),
        );
        pb.set_message(format!("Downloading {}", self.dest.file_name().unwrap().to_string_lossy()));
        pb.set_position(downloaded);

        let mut builder = self.client.get(&self.url);
        if downloaded > 0 {
            builder = builder.header(reqwest::header::RANGE, format!("bytes={downloaded}-"));
        }
        let mut resp = builder
            .send()
            .await
            .with_context(|| format!("downloading {}", self.url))?
            .error_for_status()
            .with_context(|| format!("download failed for {}", self.url))?;

        // A resumed request must be answered with 206 Partial Content.
        // If the server ignored the Range header and sent the whole file (200),
        // the existing `.partial` prefix is stale: start it over.
        if downloaded > 0 && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            tracing::debug!("server ignored Range for {}; restarting download", self.dest.display());
            tokio::fs::remove_file(&tmp).await.ok();
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

        // Verify SHA256 before atomic rename
        if !self.verify_sha256(&tmp).await? {
            let _ = tokio::fs::remove_file(&tmp).await;
            bail!(
                "checksum mismatch for {}; deleted the corrupt download",
                self.dest.display()
            );
        }

        tokio::fs::rename(&tmp, &self.dest).await?;
        pb.finish_and_clear();
        Ok(())
    }

    async fn verify_sha256(&self, path: &Path) -> Result<bool> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];
        loop {
            let n = tokio::io::AsyncReadExt::read(&mut file, &mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        let actual = hex::encode(hasher.finalize());
        Ok(actual.eq_ignore_ascii_case(&self.expected_sha256))
    }
}