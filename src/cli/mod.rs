pub mod answer;
pub mod doctor;
pub mod index;
pub mod models;
pub mod search;

use crate::core::config::Config;
use crate::core::index::{init_db, Connection};
use crate::core::models::{ModelManager, DEFAULT_EMBEDDING, MULTILINGUAL_EMBEDDING, OPTIONAL_RERANKER};
use crate::core::paths::Paths;
use crate::core::search::{SearchPipeline, SharedEmbedder};
use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

pub const EXIT_FOUND: i32 = 0;
pub const EXIT_NOT_FOUND: i32 = 1;
pub const EXIT_ERROR: i32 = 2;
pub const EXIT_MODEL_MISSING: i32 = 3;

/// Shared state for every subcommand: where things live and what is indexed.
pub struct Workspace {
    pub paths: Paths,
    pub config: Config,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            paths: Paths::new(),
            config: Config::load().unwrap_or_default(),
        }
    }

    /// Search scope: `--root` flags, then the config file, then *everything* in
    /// the index. Searching must not silently depend on the current directory.
    pub fn search_roots(&self, flags: &[String]) -> Vec<PathBuf> {
        if !flags.is_empty() {
            return flags.iter().map(PathBuf::from).collect();
        }
        self.config.roots.clone()
    }

    /// CLI `--root` flags win, then the config file, then the current directory.
    pub fn roots(&self, flags: &[String]) -> Vec<PathBuf> {
        if !flags.is_empty() {
            return flags.iter().map(PathBuf::from).collect();
        }
        if !self.config.roots.is_empty() {
            return self.config.roots.clone();
        }
        std::env::current_dir().map(|d| vec![d]).unwrap_or_default()
    }

    pub fn open_index(&self) -> Result<Connection> {
        let conn = Connection::open(&self.paths.db_path())?;
        init_db(&conn)?;
        Ok(conn)
    }

    /// Load the embedder only when it is already on disk: searching must never
    /// trigger a surprise download. If both embedding models are present the
    /// multilingual one wins, since it also covers English.
    pub fn embedder(&self) -> Option<SharedEmbedder> {
        let manager = ModelManager::new();
        for candidate in [MULTILINGUAL_EMBEDDING, DEFAULT_EMBEDDING] {
            if !manager.is_installed(candidate) {
                continue;
            }
            if let Some(embedder) = manager.embedder_if_installed(candidate) {
                return Some(std::sync::Arc::new(embedder) as SharedEmbedder);
            }
        }
        None
    }

    pub fn pipeline(&self) -> Result<SearchPipeline> {
        Ok(SearchPipeline::new(
            self.open_index()?,
            self.embedder(),
            ModelManager::new().reranker_if_installed(),
            self.config.search(),
        ))
    }

    pub fn embedding_model_installed(&self) -> bool {
        let manager = ModelManager::new();
        manager.is_installed(DEFAULT_EMBEDDING) || manager.is_installed(MULTILINGUAL_EMBEDDING)
    }

    pub fn reranker_installed(&self) -> bool {
        ModelManager::new().is_installed(OPTIONAL_RERANKER)
    }
}

#[derive(Args)]
pub struct SearchArgs {
    pub query: String,
    #[arg(long, short)]
    pub root: Vec<String>,
    #[arg(long, short, default_value = "10")]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
    /// Fail with exit code 3 if the embedding model is not installed.
    #[arg(long)]
    pub semantic: bool,
}

#[derive(Args)]
pub struct AnswerArgs {
    pub question: String,
    #[arg(long, short)]
    pub root: Vec<String>,
    #[arg(long)]
    pub polish: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct IndexArgs {
    #[arg(long, short)]
    pub root: Vec<String>,
    #[arg(long)]
    pub watch: bool,
    #[arg(long)]
    pub file: Option<String>,
    /// Also load the embedding model so semantic search covers new chunks.
    #[arg(long)]
    pub embeddings: bool,
}

#[derive(Args)]
pub struct ModelsArgs {
    #[command(subcommand)]
    pub command: ModelsCommand,
}

#[derive(Subcommand)]
pub enum ModelsCommand {
    List,
    Install { model: String },
    Remove { model: String },
}

pub async fn run_search(args: SearchArgs) -> Result<i32> {
    search::run(args).await
}

pub async fn run_answer(args: AnswerArgs) -> Result<i32> {
    answer::run(args).await
}

pub async fn run_index(args: IndexArgs) -> Result<i32> {
    index::run(args).await
}

pub async fn run_models(args: ModelsArgs) -> Result<i32> {
    models::run(args).await
}

pub async fn run_doctor() -> Result<i32> {
    doctor::run().await
}
