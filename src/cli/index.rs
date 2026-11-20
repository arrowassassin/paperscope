use super::{IndexArgs, Workspace, EXIT_FOUND, EXIT_MODEL_MISSING};
use crate::core::index::{IndexStats, Indexer};
use crate::core::models::DEFAULT_EMBEDDING;
use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub async fn run(args: IndexArgs) -> Result<i32> {
    let ws = Workspace::new();
    let conn = ws.open_index()?;
    let embedder = ws.embedder();
    if args.embeddings && embedder.is_none() {
        eprintln!("embedding model is not installed; run `paper models install {DEFAULT_EMBEDDING}`");
        return Ok(EXIT_MODEL_MISSING);
    }
    if embedder.is_none() {
        eprintln!("indexing without embeddings (keyword search only); install {DEFAULT_EMBEDDING} for semantic search");
    }

    let indexer = Indexer::with_ocr(conn, embedder, args.ocr)?;

    // One pass over the requested scope: new files are added, changed files are
    // replaced, unchanged files are skipped by mtime+size and then by hash.
    let stats = if let Some(file) = &args.file {
        indexer.index_file(&PathBuf::from(file))?
    } else {
        let roots = ws.roots(&args.root);
        indexer.index_roots(&roots)?
    };
    let pruned = indexer.prune_missing()?;
    report(&stats, pruned);

    if args.watch {
        watch(&indexer, &args)?;
        return Ok(EXIT_FOUND);
    }

    if !ws.embedding_model_installed() {
        eprintln!("hint: `paper models install {DEFAULT_EMBEDDING}` enables semantic search");
    }
    Ok(EXIT_FOUND)
}

fn report(stats: &IndexStats, pruned: usize) {
    println!(
        "indexed {} file(s) ({} chunks), {} unchanged, {} skipped, {} removed",
        stats.indexed, stats.chunks, stats.unchanged, stats.skipped, pruned
    );
    if stats.warnings > 0 {
        eprintln!(
            "{} extraction warning(s): some pages or sheets had no readable text (scans need OCR, which is opt-in)",
            stats.warnings
        );
    }
}

/// Keep the index in step with the filesystem until interrupted.
///
/// Events are debounced into batches so saving one document once does not cause
/// several reindexes, and a batch is applied in a single pass. Deletions are
/// handled by `prune_missing`, which only removes rows whose file is gone.
fn watch(indexer: &Indexer, args: &IndexArgs) -> Result<()> {
    let targets: Vec<PathBuf> = if let Some(file) = &args.file {
        vec![PathBuf::from(file)]
    } else {
        super::Workspace::new().roots(&args.root)
    };

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .context("starting filesystem watcher")?;

    for target in &targets {
        let mode = if target.is_file() {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };
        watcher
            .watch(target, mode)
            .with_context(|| format!("watching {}", target.display()))?;
    }
    eprintln!(
        "watching {} target(s); press Ctrl-C to stop",
        targets.len()
    );

    loop {
        let Ok(Ok(event)) = rx.recv() else {
            continue;
        };
        let mut changed: BTreeSet<PathBuf> = event.paths.into_iter().collect();

        // Debounce: editors emit create+modify+rename for one save.
        while let Ok(Ok(next)) = rx.recv_timeout(Duration::from_millis(250)) {
            changed.extend(next.paths);
        }

        let mut stats = IndexStats::default();
        let mut pruned = 0usize;
        for path in changed {
            if is_ignored(&path) {
                continue;
            }
            if path.is_file() {
                match indexer.index_file(&path) {
                    Ok(file_stats) => stats.merge(file_stats),
                    Err(err) => eprintln!("{}: {err}", path.display()),
                }
            } else if !path.exists() {
                // Removals are resolved once per batch, not per path.
                pruned += indexer.prune_missing()?;
            }
        }
        if stats.scanned > 0 || pruned > 0 {
            report(&stats, pruned);
        }
    }
}

fn is_ignored(path: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    let hidden = path
        .file_name()
        .map(|name| name.to_string_lossy().starts_with('.'))
        .unwrap_or(true);
    hidden || !crate::core::extract::is_supported(&path.to_string_lossy())
}
