use super::{Workspace, EXIT_FOUND};
use crate::core::index::Connection;
use crate::core::models::ModelManager;
use anyhow::Result;

const NOTICE: &str = include_str!("../../NOTICE");

pub async fn run() -> Result<i32> {
    let ws = Workspace::new();
    println!("paper doctor");
    println!("  data dir:   {}", ws.paths.base_dir().display());
    println!("  database:   {}", ws.paths.db_path().display());
    println!("  config:     {}", ws.paths.config_path().display());

    match Connection::open(&ws.paths.db_path()) {
        Ok(conn) => {
            crate::core::index::init_db(&conn)?;
            let chunks = conn.query_i64("SELECT COUNT(*) FROM chunks").unwrap_or(0);
            let files = conn
                .query_i64("SELECT COUNT(DISTINCT path) FROM chunks")
                .unwrap_or(0);
            let vectors = conn
                .query_i64("SELECT COUNT(*) FROM chunks_vec")
                .unwrap_or(0);
            let version: String = conn
                .inner()
                .query_row("SELECT vec_version()", [], |r| r.get(0))
                .unwrap_or_else(|_| "unavailable".into());
            println!("  documents:  {files} files, {chunks} chunks, {vectors} embeddings");
            println!("  sqlite-vec: {version}");
            if vectors == 0 && chunks > 0 {
                println!("  note: no embeddings yet; run `paper index --embeddings` after installing the model");
            }
        }
        Err(e) => println!("  database:   unavailable ({e})"),
    }

    for model in ModelManager::new().list() {
        println!(
            "  model:      {:<22} {} ({} MB)",
            model.id,
            if model.installed {
                "installed"
            } else {
                "not installed"
            },
            model.size_mb
        );
    }
    println!("  telemetry:  disabled (there is no telemetry code in this binary)");
    println!();
    println!("{}", NOTICE);
    Ok(EXIT_FOUND)
}
