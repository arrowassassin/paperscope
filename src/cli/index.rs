use super::{IndexArgs, Workspace, EXIT_FOUND, EXIT_MODEL_MISSING};
use crate::core::index::Indexer;
use crate::core::models::DEFAULT_EMBEDDING;
use anyhow::Result;
use std::path::PathBuf;

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

    let indexer = Indexer::new(conn, embedder)?;
    let stats = if let Some(file) = &args.file {
        indexer.index_file(&PathBuf::from(file))?
    } else {
        let roots = ws.roots(&args.root);
        indexer.index_roots(&roots)?
    };
    let pruned = indexer.prune_missing()?;

    println!(
        "indexed {} file(s) ({} chunks), {} unchanged, {} skipped, {} removed",
        stats.indexed, stats.chunks, stats.unchanged, stats.skipped, pruned
    );
    if !ws.embedding_model_installed() {
        eprintln!("hint: `paper models install {DEFAULT_EMBEDDING}` enables semantic search");
    }
    if args.watch {
        eprintln!("watch mode is not implemented yet; re-run `paper index` to refresh");
    }
    Ok(EXIT_FOUND)
}
