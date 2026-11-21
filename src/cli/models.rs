use super::{ModelsArgs, ModelsCommand, EXIT_ERROR, EXIT_FOUND};
use crate::cli::model_downloader::ModelDownloader;
use crate::core::models::ModelManager;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

/// Per-model install/removal guard.
///
/// Serializes concurrent `paper models install` / `paper models remove` calls
/// for the same model so they cannot race on the shared model directory or its
/// `.partial` download files. Different models proceed in parallel.
static MODEL_INSTALL_LOCKS: OnceLock<StdMutex<HashMap<String, Arc<StdMutex<()>>>>> =
    OnceLock::new();

/// Returns a lock unique to `model_id`, creating it on first use.
fn model_install_lock(model_id: &str) -> Arc<StdMutex<()>> {
    let mut map = MODEL_INSTALL_LOCKS
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .expect("model install lock map poisoned");
    map.entry(model_id.to_string())
        .or_insert_with(|| Arc::new(StdMutex::new(())))
        .clone()
}

pub async fn run(args: ModelsArgs) -> Result<i32> {
    let manager = ModelManager::new();
    match args.command {
        ModelsCommand::List => {
            println!(
                "{:<24} {:>7}  {:<9} {:<16} DESCRIPTION",
                "MODEL", "SIZE", "INSTALLED", "LANGUAGES"
            );
            for m in manager.list() {
                println!(
                    "{:<24} {:>5}MB  {:<9} {:<16} {}{}",
                    m.id,
                    m.size_mb,
                    if m.installed { "yes" } else { "no" },
                    m.languages,
                    m.name,
                    if m.is_default { " [default]" } else { "" }
                );
            }
        }
        ModelsCommand::Install { model } => {
            if !cfg!(feature = "embeddings") {
                eprintln!(
                    "this build has no embedding support: rebuild with --features embeddings \
                     (keyword search works without it)"
                );
                return Ok(EXIT_ERROR);
            }
            // Serialize installs of the same model so concurrent calls cannot
            // race on the model directory / `.partial` files (M14).
            let _install_guard = model_install_lock(&model);

            let spec = manager.spec(&model)?;
            let model_dir = manager.model_dir(&model);
            tokio::fs::create_dir_all(&model_dir).await?;

            // Aggregate progress bar for the whole model
            let total_size = spec.size_bytes();
            let aggregate_pb = ProgressBar::new(total_size);
            aggregate_pb.set_style(
                ProgressStyle::with_template(
                    "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
                )
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("##-"),
            );
            aggregate_pb.set_message(format!("Downloading {} ({} MB)", model, spec.size_mb()));

            for file in spec.files {
                let dest = model_dir.join(file.name);

                // GitHub Releases first, Hugging Face as fallback (see
                // `ModelSpec::download_urls`). ModelDownloader verifies the
                // same SHA256 pin regardless of source.
                let mut last_error = None;
                for url in spec.download_urls(file.name) {
                    let downloader =
                        ModelDownloader::new(url, dest.clone(), file.sha256.to_string(), file.size);
                    match downloader.download_with_progress().await {
                        Ok(()) => {
                            last_error = None;
                            break;
                        }
                        Err(e) => {
                            eprintln!("download failed, trying next source: {e:#}");
                            last_error = Some(e);
                        }
                    }
                }
                if let Some(e) = last_error {
                    return Err(e)
                        .with_context(|| format!("failed to download {}/{}", model, file.name));
                }

                // Update aggregate progress
                aggregate_pb.inc(file.size);
            }

            aggregate_pb.finish_with_message(format!(
                "Installed {} at {}",
                model,
                model_dir.display()
            ));
            println!("installed {} at {}", model, model_dir.display());

            // Update installed cache via is_installed (which caches on success)
            let _ = manager.is_installed(&model);
        }
        ModelsCommand::Remove { model } => {
            // Serialize removals of the same model so an install and a remove
            // cannot race on the model directory / `.partial` files (M14).
            let _remove_guard = model_install_lock(&model);

            if manager.spec(&model).is_err() {
                eprintln!("unknown model: {model}");
                return Ok(EXIT_ERROR);
            }
            manager.remove(&model).await?;
            println!("removed {model}");
        }
    }
    Ok(EXIT_FOUND)
}
