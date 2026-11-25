pub mod aggregate;
pub mod answer;
#[cfg(feature = "daemon")]
pub mod daemon;
pub mod doctor;
pub mod index;
pub mod model_downloader;
pub mod models;
pub mod search;
pub mod uninstall;
pub mod update;

#[cfg(feature = "daemon")]
pub use daemon::{DaemonControlArgs, DaemonControlCommand};
pub use uninstall::UninstallArgs;
pub use update::UpdateArgs;

use crate::core::config::Config;
use crate::core::index::{init_db, Connection};
use crate::core::models::{
    ModelManager, DEFAULT_EMBEDDING, MULTILINGUAL_EMBEDDING, OPTIONAL_RERANKER,
};
use crate::core::paths::Paths;
use crate::core::search::{SearchFilters, SearchPipeline, SharedEmbedder};
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
    /// Single `ModelManager`, constructed once so search/model checks don't
    /// rebuild the client and spec list on every call (M2).
    pub model_manager: ModelManager,
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
            model_manager: ModelManager::new(),
        }
    }

    /// One stderr line when the daemon is still setting up (stdout stays clean).
    pub fn note_setup_in_progress(&self) {
        if let Some(phase) = self.setup_status() {
            eprintln!(
                "note: Paper is still setting up in the background: {phase}. Results will fill in when it finishes."
            );
        }
    }

    /// Background-setup phase; the status file belongs to the daemon feature.
    pub fn setup_status(&self) -> Option<String> {
        #[cfg(feature = "daemon")]
        return crate::bootstrap::setup_status(&self.paths);
        #[cfg(not(feature = "daemon"))]
        None
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
        let manager = &self.model_manager;
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
            self.model_manager
                .reranker_if_installed(OPTIONAL_RERANKER)
                .map(std::sync::Arc::new),
            self.config.search(),
        ))
    }

    pub fn embedding_model_installed(&self) -> bool {
        if !cfg!(feature = "embeddings") {
            // A keyword-only build can never load a model, so it must not
            // advertise one as installed.
            return false;
        }
        let manager = &self.model_manager;
        manager.is_installed(DEFAULT_EMBEDDING) || manager.is_installed(MULTILINGUAL_EMBEDDING)
    }

    pub fn reranker_installed(&self) -> bool {
        self.model_manager.is_installed(OPTIONAL_RERANKER)
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
    /// Only files modified on or after this date (`YYYY-MM-DD`).
    #[arg(long)]
    pub after: Option<String>,
    /// Only files modified on or before this date (`YYYY-MM-DD`).
    #[arg(long)]
    pub before: Option<String>,
    /// Only these extensions, comma-separated (`pdf,docx`).
    #[arg(long, value_delimiter = ',')]
    pub ext: Vec<String>,
    /// Only paths containing this substring.
    #[arg(long)]
    pub path: Option<String>,
}

impl SearchArgs {
    pub fn filters(&self) -> Result<SearchFilters> {
        SearchFilters::from_parts(
            self.after.as_deref(),
            self.before.as_deref(),
            &self.ext,
            self.path.clone(),
        )
    }
}

#[derive(Args)]
pub struct AnswerArgs {
    pub question: String,
    #[arg(long, short)]
    pub root: Vec<String>,
    #[arg(long)]
    pub polish: bool,
    /// Which engine polishes the answer: `local` (an OpenAI-compatible server
    /// on this machine) or `openrouter` (opt-in cloud fallback).
    #[arg(long, default_value = "local", value_parser = ["local", "openrouter"])]
    pub engine: String,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub after: Option<String>,
    #[arg(long)]
    pub before: Option<String>,
    #[arg(long, value_delimiter = ',')]
    pub ext: Vec<String>,
    #[arg(long)]
    pub path: Option<String>,
}

impl AnswerArgs {
    pub fn filters(&self) -> Result<SearchFilters> {
        SearchFilters::from_parts(
            self.after.as_deref(),
            self.before.as_deref(),
            &self.ext,
            self.path.clone(),
        )
    }
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
    /// Force OCR for scanned pages and images (on by default when the OCR
    /// models are installed).
    #[arg(long)]
    pub ocr: bool,
    /// Skip OCR even when the models are installed.
    #[arg(long)]
    pub no_ocr: bool,
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

pub async fn run_aggregate(args: aggregate::AggregateArgs) -> Result<i32> {
    aggregate::run(args).await
}

pub async fn run_doctor() -> Result<i32> {
    doctor::run().await
}

#[cfg(feature = "daemon")]
pub async fn run_daemon_control(args: DaemonControlArgs) -> Result<i32> {
    daemon::run(args).await
}

/// Drop control characters (C0, DEL, C1) except `\n` and `\t` from text
/// headed for a terminal, so an indexed document cannot inject escape
/// sequences. Same filter as `polish::sanitize`, without the trim.
fn terminal_safe(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::terminal_safe;

    #[test]
    fn terminal_safe_drops_control_characters_and_keeps_printable_text() {
        let raw = "a\x1b[2Jb\x1b]0;pwned\x07c\u{9b}31md\x7fe\r\nf\tg  ";
        assert_eq!(terminal_safe(raw), "a[2Jb]0;pwnedc31mde\nf\tg  ");
        assert_eq!(terminal_safe("Grüße, 世界"), "Grüße, 世界");
    }
}
