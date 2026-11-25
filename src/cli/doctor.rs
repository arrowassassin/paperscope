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

    if let Some(phase) = ws.setup_status() {
        println!("  setup:      in progress, {phase}");
    }

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
        Err(e) => println!("  database:   unavailable ({e:#})"),
    }

    let manager = ModelManager::new();
    for model in manager.list() {
        println!(
            "  model:      {:<22} {} ({} MB)",
            model.id,
            if model.installed {
                // `installed` is a cheap size check; doctor does the full hash.
                if manager.is_verified(&model.id) {
                    "installed"
                } else {
                    "installed but FAILS CHECKSUM (run `paper models install`)"
                }
            } else {
                "not installed"
            },
            model.size_mb
        );
    }

    #[cfg(feature = "ocr")]
    {
        let present = crate::core::extract::ocr::models_present();
        let state = if crate::core::extract::ocr::available() {
            "enabled"
        } else if std::env::var_os("PAPER_NO_OCR").is_some() {
            "disabled by PAPER_NO_OCR"
        } else {
            "waiting for models"
        };
        println!(
            "  ocr:        {state} (paddleocr det+rec, {}); models at {}",
            if present {
                "installed"
            } else {
                "not installed"
            },
            crate::core::extract::ocr::model_dir().display()
        );
    }
    #[cfg(not(feature = "ocr"))]
    println!("  ocr:        not compiled into this build");

    println!("  telemetry:  disabled (there is no telemetry code in this binary)");
    crate::cli::update::warn_if_shadowed();

    println!();
    println!("{}", NOTICE);
    Ok(EXIT_FOUND)
}
