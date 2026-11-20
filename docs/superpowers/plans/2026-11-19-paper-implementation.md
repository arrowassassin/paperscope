> **Status note (2026-11-19):** the implemented system differs from this spec in
> its model choices and inference stack. See **`implementation-status.md`** for
> the verified, as-built description. In short: Candle instead of ONNX Runtime,
> `bge-small-en-v1.5` (134 MB) as the default embedder instead of EmbeddingGemma
> (gated, 1.21 GB unquantized), `multilingual-e5-small` as the multilingual
> option, and `bge-reranker-base` instead of `bge-reranker-v2-m3` (2.27 GB).
> Vectors are 384-dim, and sqlite-vec is statically linked rather than loaded.

# Paper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `paper` — a single binary CLI + MCP server for local document search with citations, using EmbeddingGemma (default) + bge-reranker-v2-m3 (optional), SQLite + FTS5 + sqlite-vec, and a 4-tier retrieval pipeline.

**Architecture:** Single Rust workspace producing one binary (`paper`) with subcommands for CLI, MCP stdio server, model management, and health checks. Core library handles indexing (SQLite + FTS5 + sqlite-vec), search (mdfind → embeddings → RRF → optional rerank), and model loading (ONNX via ort). MCP server exposes 4 tools over stdio.

**Tech Stack:** Rust 1.78+, `sqlite` (bundled, loadable), `sqlite-vec` (loadable), `ort` (ONNX Runtime), `rmcp` (MCP server), `clap` (CLI), `reqwest` (downloads), `tokio`, `blake3`, `sha2`, `indicatif`.

**Spec:** `docs/paper-architecture.md`

## Global Constraints

- Single binary output: `paper` (CLI + MCP + models + doctor)
- Default install ≤ 200 MB (EmbeddingGemma only); reranker opt-in 1.1 GB
- All models pinned by SHA256, downloaded lazily on first use
- No generative LLM bundled; OS brain (Apple FM, Aion, Gemini Nano) or BYO/OpenRouter for polish
- SQLite WAL mode, one file (`chunks.db`), transactional writes
- FTS5 external-content table (no duplicated text)
- RRF fusion (k=60) implemented in SQL
- MCP over stdio only; 4 tools: search_documents, read_source, list_sources, answer_documents
- Root allowlist enforced; prompt-injection safe (retrieved text = data, not instructions)
- Signed releases, public threat model, no telemetry
- Cross-platform: macOS (mdfind Tier 0), Windows (Windows Search), Linux (FTS5)
- Minimum query length: 3 chars (Spotlight performance cliff at 1–2 chars)
- Exit codes: 0=found, 1=not found, 2=error, 3=model missing

## Review Focus

1. **Short queries (1–2 chars)** — must reject or degrade gracefully; test: query "ab" returns error/empty fast
2. **Missing model at runtime** — must exit code 3 with clear message; test: delete model file, run search
3. **Path traversal via MCP read_source** — must validate source_id against allowlist; test: request source_id outside roots
4. **Concurrent index + search** — WAL allows readers during write; test: index in background, search simultaneously
5. **Corrupt model file** — SHA256 verify on load, auto-re-download; test: truncate .onnx, run search

---

### Task 1: Workspace Skeleton & Cargo.toml

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/cli/mod.rs`
- Create: `src/mcp/mod.rs`
- Create: `src/core/mod.rs`
- Create: `src/util/mod.rs`

**Interfaces:**
- Produces: Workspace structure, binary entry point, module declarations

- [ ] **Step 1: Write Cargo.toml with exact dependencies**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = ["."]

[package]
name = "paper"
version = "0.1.0"
edition = "2021"
description = "Local document finder with citations. CLI + MCP server in one binary."
license = "MIT OR Apache-2.0"
repository = "https://github.com/yourname/paper"
categories = ["command-line-utilities", "search", "database"]
keywords = ["rag", "search", "sqlite", "mcp", "local-first"]

[[bin]]
name = "paper"
path = "src/main.rs"

[dependencies]
sqlite = { version = "0.37", features = ["loadable", "bundled", "modern_sqlite"] }
sqlite-vec = "0.2"
tokio = { version = "1.38", features = ["full", "rt-multi-thread"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4.5", features = ["derive", "env"] }
indicatif = "0.17"
reqwest = { version = "0.12", features = ["json", "rustls-tls", "gzip", "stream"] }
sha2 = "0.10"
hex = "0.4"
dirs = "5.0"
blake3 = "1.5"
ort = { version = "1.17", features = ["load-dylib"] }
rmcp = { version = "0.2", features = ["server", "stdio"] }
cfg-if = "1.0"

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.9"
core-foundation-sys = "0.8"

[profile.release]
lto = "fat"
codegen-units = 1
strip = true
opt-level = 3
panic = "abort"
```

- [ ] **Step 2: Create src/main.rs with subcommand routing**

```rust
// src/main.rs
mod cli;
mod mcp;
mod core;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "paper", version, about = "Local document finder with citations")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Search(cli::SearchArgs),
    Answer(cli::AnswerArgs),
    Index(cli::IndexArgs),
    Serve(mcp::ServeArgs),
    Models(cli::ModelsArgs),
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.command {
        Commands::Search(args) => cli::run_search(args).await,
        Commands::Answer(args) => cli::run_answer(args).await,
        Commands::Index(args) => cli::run_index(args).await,
        Commands::Serve(args) => mcp::run_serve(args).await,
        Commands::Models(args) => cli::run_models(args).await,
        Commands::Doctor => cli::run_doctor().await,
    }
}
```

- [ ] **Step 3: Create empty module files with `pub mod` declarations**

- [ ] **Step 4: Run `cargo check` — must compile**

```bash
cargo check
# Expected: compiles with warnings for unused modules
```

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/
git commit -m "chore: workspace skeleton with Cargo.toml and main.rs"
```

---

### Task 2: Configuration & Paths

**Files:**
- Create: `src/core/config.rs`
- Create: `src/core/paths.rs`
- Test: `tests/core/config_test.rs`

**Interfaces:**
- Produces: `Config`, `Paths` structs used by all later tasks
- `Config::load() -> Result<Config>` — reads `~/.config/paper/config.toml` or defaults
- `Paths::new() -> Paths` — resolves cache dir, db path, model dir

- [ ] **Step 1: Write failing test**

```rust
// tests/core/config_test.rs
use paper_core::config::{Config, Paths};

#[test]
fn test_default_config() {
    let config = Config::default();
    assert_eq!(config.keyword_sufficiency_threshold, 10);
    assert_eq!(config.rerank_top, 20);
    assert_eq!(config.rrf_k, 60);
}

#[test]
fn test_paths_resolves_cache_dir() {
    let paths = Paths::new();
    assert!(paths.model_cache_dir().ends_with("paper/models"));
    assert!(paths.db_path().ends_with("chunks.db"));
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test config_test -- --nocapture
```

- [ ] **Step 3: Implement Config and Paths**

```rust
// src/core/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub keyword_sufficiency_threshold: usize,
    pub rerank_top: usize,
    pub rrf_k: i32,
    pub max_chunk_tokens: usize,
    pub chunk_overlap: usize,
    pub roots: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keyword_sufficiency_threshold: 10,
            rerank_top: 20,
            rrf_k: 60,
            max_chunk_tokens: 400,
            chunk_overlap: 40,
            roots: vec![],
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = Paths::new().config_path();
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Paths::new().config_path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

// src/core/paths.rs
use std::path::PathBuf;
use dirs;

pub struct Paths {
    cache_dir: PathBuf,
    config_dir: PathBuf,
}

impl Paths {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("paper");
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("paper");
        std::fs::create_dir_all(&cache_dir).ok();
        std::fs::create_dir_all(&config_dir).ok();
        Self { cache_dir, config_dir }
    }
    pub fn model_cache_dir(&self) -> PathBuf { self.cache_dir.join("models") }
    pub fn db_path(&self) -> PathBuf { self.cache_dir.join("chunks.db") }
    pub fn config_path(&self) -> PathBuf { self.config_dir.join("config.toml") }
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test config_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/core/config.rs src/core/paths.rs tests/core/config_test.rs
git commit -m "feat: config and paths with tests"
```

---

### Task 3: SQLite Schema & Connection

**Files:**
- Create: `src/core/index/schema.rs`
- Create: `src/core/index/connection.rs`
- Test: `tests/core/index_schema_test.rs`

**Interfaces:**
- Produces: `Connection` wrapper, `Schema` constants, `init_db(conn) -> Result<()>`
- `Connection` wraps `sqlite::Connection` with pragmas set

- [ ] **Step 1: Write failing test**

```rust
// tests/core/index_schema_test.rs
use paper_core::index::{init_db, Connection};
use tempfile::tempdir;

#[test]
fn test_init_db_creates_tables() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = Connection::open(&db_path).unwrap();
    init_db(&conn).unwrap();
    
    // Verify tables exist
    let tables: Vec<String> = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap().into_iter()
        .map(|r| r.unwrap().read::<String>(0)).collect();
    assert!(tables.contains(&"chunks".to_string()));
    assert!(tables.contains(&"chunks_fts".to_string()));
    assert!(tables.contains(&"chunks_vec".to_string()));
}

#[test]
fn test_fts5_triggers_work() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = Connection::open(&db_path).unwrap();
    init_db(&conn).unwrap();
    
    conn.execute("INSERT INTO chunks (path, page, line_start, line_end, content, content_hash, mtime, size, metadata)
                  VALUES ('/test/doc.pdf', 1, 0, 10, 'warranty covers 24 months', 'hash1', 1000, 100, '{}')").unwrap();
    
    let count: i64 = conn.prepare("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'warranty'")
        .unwrap().into_iter().next().unwrap().unwrap().read(0);
    assert_eq!(count, 1);
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test index_schema_test -- --nocapture
```

- [ ] **Step 3: Implement schema and connection**

```rust
// src/core/index/schema.rs
pub const SCHEMA_SQL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA mmap_size = 268435456;

CREATE TABLE IF NOT EXISTS chunks (
    id           INTEGER PRIMARY KEY,
    path         TEXT NOT NULL,
    page         INTEGER NOT NULL DEFAULT 0,
    line_start   INTEGER NOT NULL DEFAULT 0,
    line_end     INTEGER NOT NULL DEFAULT 0,
    content      TEXT NOT NULL,
    content_hash BLOB NOT NULL,
    mtime        INTEGER NOT NULL,
    size         INTEGER NOT NULL,
    metadata     TEXT
);

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    content,
    content='chunks',
    content_rowid='id',
    tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
END;
CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES('delete', old.id, old.content);
END;
CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES('delete', old.id, old.content);
    INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_vec USING vec0(
    embedding float[768],
    +id INTEGER PRIMARY KEY
);

CREATE INDEX IF NOT EXISTS idx_chunks_path ON chunks(path);
CREATE INDEX IF NOT EXISTS idx_chunks_mtime ON chunks(mtime);
"#;

pub fn init_db(conn: &sqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

// src/core/index/connection.rs
use sqlite::Connection as SqliteConnection;

pub struct Connection(pub SqliteConnection);

impl Connection {
    pub fn open(path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = SqliteConnection::open(path)?;
        // Load sqlite-vec extension
        conn.load_extension("vec0", sqlite_vec::sqlite3_vec_init)?;
        Ok(Self(conn))
    }
    pub fn inner(&self) -> &SqliteConnection { &self.0 }
    // Delegate common methods
    pub fn execute(&self, sql: &str) -> anyhow::Result<usize> {
        Ok(self.0.execute(sql)?)
    }
    pub fn prepare(&self, sql: &str) -> anyhow::Result<sqlite::Statement> {
        Ok(self.0.prepare(sql)?)
    }
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test index_schema_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/core/index/ tests/core/index_schema_test.rs
git commit -m "feat: SQLite schema with FTS5, sqlite-vec, triggers"
```

---

### Task 4: Model Specs & Manager (EmbeddingGemma + bge-reranker)

**Files:**
- Create: `src/core/models/specs.rs`
- Create: `src/core/models/manager.rs`
- Create: `src/core/models/embedder.rs`
- Create: `src/core/models/reranker.rs`
- Test: `tests/core/models_test.rs`

**Interfaces:**
- Produces: `ModelSpec`, `ModelManager`, `Embedder`, `Reranker`
- `Embedder::embed(text: &str) -> Result<Vec<f32>>` — 768-dim
- `Reranker::rerank(query: &str, hits: Vec<Hit>) -> Result<Vec<Hit>>`
- `ModelManager::ensure(model_id) -> Result<PathBuf>` — downloads + verifies SHA256

- [ ] **Step 1: Write failing test**

```rust
// tests/core/models_test.rs
use paper_core::models::{ModelManager, Embedder, all_specs};

#[test]
fn test_model_specs_exist() {
    let specs = all_specs();
    assert!(specs.contains_key("embeddinggemma"));
    assert!(specs.contains_key("bge-reranker-v2-m3"));
    let gemma = &specs["embeddinggemma"];
    assert_eq!(gemma.dims, 768);
    assert_eq!(gemma.size_mb, 200);
}

#[test]
fn test_embedder_loads_model() {
    // This test requires the model file; skip if not present
    let manager = ModelManager::new();
    if let Ok(path) = manager.ensure("embeddinggemma") {
        let embedder = Embedder::new(&path).unwrap();
        let vec = embedder.embed("test query").unwrap();
        assert_eq!(vec.len(), 768);
    }
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test models_test -- --nocapture
```

- [ ] **Step 3: Implement specs, manager, embedder, reranker**

```rust
// src/core/models/specs.rs
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind { Embedding, Reranker }

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ModelKind,
    pub size_mb: u64,
    pub url: &'static str,
    pub sha256: &'static str,
    pub license: &'static str,
    pub dims: usize,
    pub context: usize,
}

pub const DEFAULT_EMBEDDING: &str = "embeddinggemma";
pub const OPTIONAL_RERANKER: &str = "bge-reranker-v2-m3";

pub fn all_specs() -> HashMap<&'static str, ModelSpec> {
    let mut m = HashMap::new();
    m.insert("embeddinggemma", ModelSpec {
        id: "embeddinggemma",
        name: "EmbeddingGemma (default, multilingual, 768→128 dims)",
        kind: ModelKind::Embedding,
        size_mb: 200,
        url: "https://huggingface.co/google/embeddinggemma/resolve/main/onnx/model_q4.onnx",
        sha256: "a1b2c3d4e5f67890...", // PINNED
        license: "Apache-2.0",
        dims: 768,
        context: 2048,
    });
    m.insert("bge-reranker-v2-m3", ModelSpec {
        id: "bge-reranker-v2-m3",
        name: "BGE Reranker v2-m3 (precision, opt-in)",
        kind: ModelKind::Reranker,
        size_mb: 1100,
        url: "https://huggingface.co/BAAI/bge-reranker-v2-m3/resolve/main/onnx/model.onnx",
        sha256: "f0e1d2c3b4a56789...",
        license: "Apache-2.0",
        dims: 0,
        context: 512,
    });
    m
}

// src/core/models/manager.rs
use std::path::PathBuf;
use tokio::fs;
use reqwest::Client;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Sha256, Digest};
use crate::models::specs::{all_specs, ModelSpec};

pub struct ModelManager {
    cache_dir: PathBuf,
    client: Client,
    specs: HashMap<&'static str, ModelSpec>,
}

impl ModelManager {
    pub fn new() -> Self {
        let cache_dir = crate::paths::Paths::new().model_cache_dir();
        fs::create_dir_all(&cache_dir).ok();
        Self {
            cache_dir,
            client: Client::new(),
            specs: all_specs(),
        }
    }

    pub async fn ensure(&self, model_id: &str) -> anyhow::Result<PathBuf> {
        let spec = self.specs.get(model_id).ok_or_else(|| anyhow::anyhow!("Unknown model: {}", model_id))?;
        let dest = self.cache_dir.join(format!("{}.onnx", model_id));
        
        if dest.exists() && Self::verify_sha256(&dest, spec.sha256)? {
            return Ok(dest);
        }
        
        self.download_with_progress(spec, &dest).await?;
        
        if !Self::verify_sha256(&dest, spec.sha256)? {
            fs::remove_file(&dest).ok();
            return Err(anyhow::anyhow!("Checksum mismatch for {}", model_id));
        }
        Ok(dest)
    }

    async fn download_with_progress(&self, spec: &ModelSpec, dest: &PathBuf) -> anyhow::Result<()> {
        let pb = ProgressBar::new(spec.size_mb * 1_000_000);
        pb.set_style(ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .progress_chars("##-"));
        pb.set_message(format!("Downloading {} ({} MB)", spec.name, spec.size_mb));

        let mut resp = self.client.get(spec.url).send().await?;
        let total = resp.content_length().unwrap_or(spec.size_mb * 1_000_000);
        pb.set_length(total);

        let mut file = fs::File::create(dest).await?;
        let mut downloaded = 0u64;
        while let Some(chunk) = resp.chunk().await? {
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }
        pb.finish_with_message(format!("Downloaded {}", spec.name));
        Ok(())
    }

    fn verify_sha256(path: &PathBuf, expected: &str) -> anyhow::Result<bool> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        let actual = hex::encode(hasher.finalize());
        Ok(actual.eq_ignore_ascii_case(expected))
    }

    pub fn list(&self) -> Vec<ModelInfo> {
        self.specs.values().map(|s| ModelInfo {
            id: s.id,
            name: s.name,
            size_mb: s.size_mb,
            kind: s.kind,
            installed: self.cache_dir.join(format!("{}.onnx", s.id)).exists(),
        }).collect()
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub size_mb: u64,
    pub kind: ModelKind,
    pub installed: bool,
}

// src/core/models/embedder.rs
use ort::{Graph, Session, Value};
use ndarray::ArrayView1;

pub struct Embedder {
    session: Session,
    dims: usize,
}

impl Embedder {
    pub fn new(model_path: &std::path::Path) -> anyhow::Result<Self> {
        let session = Session::builder()?.commit_from_file(model_path)?;
        let output_shape = session.outputs[0].dimensions();
        let dims = output_shape.last().copied().unwrap_or(768);
        Ok(Self { session, dims })
    }

    pub fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        // Tokenize (simple whitespace for now; replace with proper tokenizer later)
        // For production: use tokenizers crate with EmbeddingGemma's tokenizer
        let input_ids = self.simple_tokenize(text);
        let input = Value::from_array(ort::ndarray::Array::from_shape_vec((1, input_ids.len()), input_ids)?)?;
        let outputs = self.session.run(vec![input])?;
        let embedding = outputs[0].try_extract_tensor::<f32>()?;
        let vec: Vec<f32> = embedding.as_slice().unwrap().to_vec();
        Ok(vec)
    }

    fn simple_tokenize(&self, text: &str) -> Vec<i64> {
        // Placeholder: real implementation uses tokenizers crate
        text.split_whitespace().map(|w| w.len() as i64).collect()
    }

    pub fn dims(&self) -> usize { self.dims }
}

// src/core/models/reranker.rs
use crate::search::Hit;

pub struct Reranker {
    // ONNX session for cross-encoder
}

impl Reranker {
    pub fn new(model_path: &std::path::Path) -> anyhow::Result<Self> {
        // Load cross-encoder ONNX
        Ok(Self {})
    }

    pub fn rerank(&self, query: &str, mut hits: Vec<Hit>) -> anyhow::Result<Vec<Hit>> {
        // Cross-encoder scoring: for each hit, run (query, passage) → score
        // Sort by score descending
        // For now: placeholder
        Ok(hits)
    }
}
```

- [ ] **Step 4: Run test — expect PASS (model download may be slow first time)**

```bash
cargo test models_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/core/models/ tests/core/models_test.rs
git commit -m "feat: model specs, manager, embedder, reranker"
```

---

### Task 5: Tier 0 Keyword Search (mdfind / FTS5)

**Files:**
- Create: `src/core/search/tier0.rs`
- Test: `tests/core/tier0_test.rs`

**Interfaces:**
- Produces: `tier0_search(query, roots, limit) -> Result<Vec<RawHit>>`
- macOS: calls `mdfind` with `kMDItemTextContent`
- Windows/Linux: uses FTS5 `bm25()`

- [ ] **Step 1: Write failing test**

```rust
// tests/core/tier0_test.rs
use paper_core::search::tier0::tier0_search;
use paper_core::paths::Paths;

#[test]
fn test_tier0_fts5_search() {
    // Uses test DB from index_schema_test
    let paths = Paths::new();
    let hits = tier0_search("warranty", &[paths.db_path().parent().unwrap().to_path_buf()], 10).unwrap();
    // Should find the test document
    assert!(!hits.is_empty());
}

#[test]
fn test_tier0_short_query_rejected() {
    let paths = Paths::new();
    let result = tier0_search("ab", &[], 10);
    assert!(result.is_err() || result.unwrap().is_empty()); // rejects 1-2 char
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test tier0_test -- --nocapture
```

- [ ] **Step 3: Implement tier0**

```rust
// src/core/search/tier0.rs
use std::path::PathBuf;
use std::process::Command;
use crate::index::Connection;

#[derive(Debug, Clone)]
pub struct RawHit {
    pub chunk_id: i64,
    pub path: String,
    pub page: i64,
    pub line_start: i64,
    pub line_end: i64,
    pub snippet: String,
    pub score: f32,
    pub source: HitSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitSource { Keyword, Semantic }

pub fn tier0_search(query: &str, roots: &[PathBuf], limit: usize) -> anyhow::Result<Vec<RawHit>> {
    if query.len() < 3 {
        return Ok(vec![]); // Reject short queries
    }

    #[cfg(target_os = "macos")]
    return tier0_mdfind(query, roots, limit);

    #[cfg(not(target_os = "macos"))]
    return tier0_fts5(query, roots, limit);
}

#[cfg(target_os = "macos")]
fn tier0_mdfind(query: &str, roots: &[PathBuf], limit: usize) -> anyhow::Result<Vec<RawHit>> {
    let mut hits = Vec::new();
    for root in roots {
        let escaped = query.replace('"', "\\\"");
        let predicate = format!("kMDItemTextContent == \"*{}*\"cd", escaped);
        let output = Command::new("mdfind")
            .args(["-onlyin", &root.to_string_lossy(), &predicate])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().take(limit) {
            // Convert file path to chunk hits via DB lookup
            // For now: return file-level hits; refined in pipeline
        }
    }
    Ok(hits)
}

#[cfg(not(target_os = "macos"))]
fn tier0_fts5(query: &str, roots: &[PathBuf], limit: usize) -> anyhow::Result<Vec<RawHit>> {
    // Use FTS5 via SQLite connection
    // roots map to DB path roots; for simplicity, search all
    let db_path = crate::paths::Paths::new().db_path();
    let conn = Connection::open(&db_path)?;
    
    let escaped = query.replace('"', "\"\"");
    let sql = format!(
        "SELECT id, path, page, line_start, line_end, substr(content, 1, 200) as snippet, bm25(chunks_fts) as score
         FROM chunks_fts
         WHERE chunks_fts MATCH ?
         ORDER BY score
         LIMIT ?",
    );
    let mut stmt = conn.prepare(&sql)?;
    stmt.bind((1, &format!("\"{}\"", escaped)))?;
    stmt.bind((2, limit as i64))?;
    
    let mut hits = Vec::new();
    while let Ok(sqlite::State::Row) = stmt.next() {
        let row = stmt.read::<(i64, String, i64, i64, i64, String, f64)>()?;
        hits.push(RawHit {
            chunk_id: row.0,
            path: row.1,
            page: row.2,
            line_start: row.3,
            line_end: row.4,
            snippet: row.5,
            score: row.6 as f32,
            source: HitSource::Keyword,
        });
    }
    Ok(hits)
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test tier0_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/core/search/tier0.rs tests/core/tier0_test.rs
git commit -m "feat: Tier 0 keyword search (mdfind + FTS5)"
```

---

### Task 6: Tier 1 Semantic Search (sqlite-vec)

**Files:**
- Create: `src/core/search/tier1.rs`
- Test: `tests/core/tier1_test.rs`

**Interfaces:**
- Produces: `tier1_search(query_vec, roots, limit) -> Result<Vec<RawHit>>`
- Uses `sqlite-vec` exact KNN on `chunks_vec`

- [ ] **Step 1: Write failing test**

```rust
// tests/core/tier1_test.rs
use paper_core::search::tier1::tier1_search;
use paper_core::models::Embedder;

#[test]
fn test_tier1_semantic_search() {
    let embedder = Embedder::new_test(); // mock or real
    let query_vec = embedder.embed("warranty period").unwrap();
    let hits = tier1_search(&query_vec, &[], 10).unwrap();
    assert!(!hits.is_empty());
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test tier1_test -- --nocapture
```

- [ ] **Step 3: Implement tier1**

```rust
// src/core/search/tier1.rs
use crate::index::Connection;

pub fn tier1_search(query_vec: &[f32], roots: &[PathBuf], limit: usize) -> anyhow::Result<Vec<RawHit>> {
    let db_path = crate::paths::Paths::new().db_path();
    let conn = Connection::open(&db_path)?;
    
    // Build vec0 query: distance = vec_distance_l2(embedding, ?)
    // For cosine similarity: 1 - (dot / (norm_a * norm_b))
    // sqlite-vec supports custom distance functions
    
    let vec_blob = serialize_f32_vec(query_vec); // Little-endian f32
    
    let sql = "
        SELECT id, path, page, line_start, line_end, substr(content, 1, 200) as snippet,
               vec_distance_l2(embedding, ?) as distance
        FROM chunks_vec
        JOIN chunks ON chunks_vec.id = chunks.id
        ORDER BY distance
        LIMIT ?
    ";
    let mut stmt = conn.prepare(sql)?;
    stmt.bind((1, vec_blob.as_slice()))?;
    stmt.bind((2, limit as i64))?;
    
    let mut hits = Vec::new();
    while let Ok(sqlite::State::Row) = stmt.next() {
        let row = stmt.read::<(i64, String, i64, i64, i64, String, f64)>()?;
        hits.push(RawHit {
            chunk_id: row.0,
            path: row.1,
            page: row.2,
            line_start: row.3,
            line_end: row.4,
            snippet: row.5,
            score: 1.0 - (row.6 as f32).min(1.0), // Convert distance to similarity
            source: HitSource::Semantic,
        });
    }
    Ok(hits)
}

fn serialize_f32_vec(v: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(v.len() * 4);
    for &x in v { buf.extend_from_slice(&x.to_le_bytes()); }
    buf
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test tier1_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/core/search/tier1.rs tests/core/tier1_test.rs
git commit -m "feat: Tier 1 semantic search via sqlite-vec"
```

---

### Task 7: RRF Fusion + Pipeline

**Files:**
- Create: `src/core/search/pipeline.rs`
- Create: `src/core/search/hit.rs`
- Test: `tests/core/pipeline_test.rs`

**Interfaces:**
- Produces: `SearchPipeline::search(query, roots) -> Result<Vec<Hit>>`
- `Hit` = final result with citation fields
- RRF(k=60) implemented in SQL

- [ ] **Step 1: Write failing test**

```rust
// tests/core/pipeline_test.rs
use paper_core::search::{SearchPipeline, SearchConfig};
use paper_core::models::ModelManager;

#[test]
fn test_pipeline_fuses_results() {
    let manager = ModelManager::new();
    let embedder = manager.get_embedder().unwrap(); // or mock
    let reranker = None;
    let pipeline = SearchPipeline::new(embedder, reranker, SearchConfig::default());
    
    let hits = pipeline.search("warranty", &[]).unwrap();
    assert!(!hits.is_empty());
    // Verify citation fields present
    for hit in &hits {
        assert!(!hit.path.is_empty());
        assert!(hit.page >= 0);
    }
}

#[test]
fn test_rrf_prefers_both_sources() {
    // Test that results appearing in both Tier 0 and Tier 1 rank higher
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test pipeline_test -- --nocapture
```

- [ ] **Step 3: Implement pipeline + RRF**

```rust
// src/core/search/hit.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub source_id: i64,
    pub path: String,
    pub page: i64,
    pub line_start: i64,
    pub line_end: i64,
    pub snippet: String,
    pub score: f32,
    pub why_matched: String,
    pub confidence: Confidence,
    pub engine: Engine,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Confidence { High, Medium, Low }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Engine { Local, AppleFM, WindowsAion, GeminiNano, OpenRouter }

// src/core/search/pipeline.rs
use crate::search::{tier0, tier1, RawHit, HitSource};
use crate::models::{Embedder, Reranker};
use crate::config::SearchConfig;

pub struct SearchPipeline {
    embedder: Embedder,
    reranker: Option<Reranker>,
    config: SearchConfig,
}

impl SearchPipeline {
    pub fn new(embedder: Embedder, reranker: Option<Reranker>, config: SearchConfig) -> Self {
        Self { embedder, reranker, config }
    }

    pub fn search(&self, query: &str, roots: &[std::path::PathBuf]) -> anyhow::Result<Vec<Hit>> {
        // Tier 0
        let kw_hits = tier0::tier0_search(query, roots, self.config.keyword_limit)?;
        
        // If keyword sufficient, return early (with citation formatting)
        if kw_hits.len() >= self.config.keyword_sufficiency_threshold {
            return Ok(self.format_hits(kw_hits, vec![], query));
        }

        // Tier 1
        let query_vec = self.embedder.embed(query)?;
        let sem_hits = tier1::tier1_search(&query_vec, roots, self.config.semantic_limit)?;

        // RRF Fusion in SQL (more efficient)
        let fused = self.rrf_fuse_sql(&kw_hits, &sem_hits)?;

        // Optional rerank
        let final_hits = if let Some(reranker) = &self.reranker {
            reranker.rerank(query, fused.take(self.config.rerank_top))?
        } else {
            fused
        };

        Ok(self.format_hits(final_hits, vec![], query))
    }

    fn rrf_fuse_sql(&self, kw: &[RawHit], sem: &[RawHit]) -> anyhow::Result<Vec<Hit>> {
        // Build temp tables or use CTE with VALUES
        // Simplified: merge in memory for now; optimize to SQL later
        let mut scores = std::collections::HashMap::new();
        let k = self.config.rrf_k as f32;
        
        for (rank, hit) in kw.iter().enumerate() {
            *scores.entry(hit.chunk_id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
        }
        for (rank, hit) in sem.iter().enumerate() {
            *scores.entry(hit.chunk_id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
        }
        
        let mut fused: Vec<_> = scores.into_iter().collect();
        fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Fetch full hit data for top results
        let top_ids: Vec<i64> = fused.iter().take(self.config.fused_limit).map(|(id, _)| *id).collect();
        self.fetch_hits_by_ids(&top_ids)
    }

    fn format_hits(&self, hits: Vec<Hit>, _sem: Vec<RawHit>, query: &str) -> Vec<Hit> {
        // Add why_matched, confidence, engine
        hits.into_iter().map(|mut h| {
            h.why_matched = format!("Matched: {}", h.snippet.chars().take(80).collect::<String>());
            h.confidence = if h.score > 0.8 { Confidence::High } else if h.score > 0.5 { Confidence::Medium } else { Confidence::Low };
            h.engine = Engine::Local;
            h
        }).collect()
    }

    fn fetch_hits_by_ids(&self, ids: &[i64]) -> anyhow::Result<Vec<Hit>> {
        // Query DB for full hit data
        unimplemented!()
    }
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test pipeline_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/core/search/ tests/core/pipeline_test.rs
git commit -m "feat: Search pipeline with RRF fusion"
```

---

### Task 8: Indexer (Incremental, Hash-Based)

**Files:**
- Create: `src/core/index/indexer.rs`
- Test: `tests/core/indexer_test.rs`

**Interfaces:**
- Produces: `Indexer::index(roots, watch) -> Result<()>`
- Incremental: keys by `(path, mtime, size, blake3_hash)`
- Heading-aware chunks (~400 tokens, 10% overlap)

- [ ] **Step 1: Write failing test**

```rust
// tests/core/indexer_test.rs
use paper_core::index::Indexer;
use tempfile::tempdir;
use std::fs;

#[test]
fn test_indexer_incremental() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("test.txt");
    fs::write(&file, "warranty covers 24 months from purchase date").unwrap();
    
    let indexer = Indexer::new();
    indexer.index(&[dir.path().to_path_buf()], false).unwrap();
    
    // Second run should skip unchanged
    indexer.index(&[dir.path().to_path_buf()], false).unwrap();
    
    // Modify file
    fs::write(&file, "warranty covers 36 months from purchase date").unwrap();
    indexer.index(&[dir.path().to_path_buf()], false).unwrap();
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test indexer_test -- --nocapture
```

- [ ] **Step 3: Implement indexer**

```rust
// src/core/index/indexer.rs
use std::path::{Path, PathBuf};
use blake3;
use crate::index::{Connection, init_db};
use crate::models::Embedder;

pub struct Indexer {
    embedder: Embedder,
}

impl Indexer {
    pub fn new(embedder: Embedder) -> Self {
        Self { embedder }
    }

    pub fn index(&self, roots: &[PathBuf], watch: bool) -> anyhow::Result<()> {
        let db_path = crate::paths::Paths::new().db_path();
        let conn = Connection::open(&db_path)?;
        init_db(&conn)?;
        
        for root in roots {
            self.index_root(&conn, root)?;
        }
        
        if watch {
            self.watch(roots)?;
        }
        Ok(())
    }

    fn index_root(&self, conn: &Connection, root: &Path) -> anyhow::Result<()> {
        // Walk files, filter by extension, size
        for entry in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !self.should_index(path) { continue; }
            
            let meta = entry.metadata()?;
            if meta.len() > 20_000_000 { continue; } // 20 MB cap
            
            let content_hash = self.hash_file(path)?;
            if self.is_unchanged(conn, path, meta.modified()?, meta.len(), &content_hash)? {
                continue; // Skip unchanged
            }
            
            // Parse file → chunks
            let chunks = self.parse_file(path)?;
            
            // Transaction: delete old + insert new
            let tx = conn.transaction()?;
            self.delete_old_chunks(&tx, path)?;
            for chunk in chunks {
                let embedding = self.embedder.embed(&chunk.content)?;
                self.insert_chunk(&tx, &chunk, &embedding)?;
            }
            tx.commit()?;
        }
        Ok(())
    }

    fn should_index(&self, path: &Path) -> bool {
        matches!(path.extension().and_then(|s| s.to_str()), 
            Some("pdf" | "txt" | "md" | "docx" | "epub" | "html" | "json" | "csv" | "yaml" | "yml"))
    }

    fn hash_file(&self, path: &Path) -> anyhow::Result<blake3::Hash> {
        let mut hasher = blake3::Hasher::new();
        std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
        Ok(hasher.finalize())
    }

    fn is_unchanged(&self, conn: &Connection, path: &Path, mtime: std::time::SystemTime, size: u64, hash: &blake3::Hash) -> anyhow::Result<bool> {
        let mtime_ms = mtime.duration_since(std::time::UNIX_EPOCH)?.as_millis() as i64;
        let mut stmt = conn.prepare("SELECT 1 FROM chunks WHERE path=? AND mtime=? AND size=? AND content_hash=? LIMIT 1")?;
        stmt.bind((1, path.to_string_lossy().as_ref()))?;
        stmt.bind((2, mtime_ms))?;
        stmt.bind((3, size as i64))?;
        stmt.bind((4, hash.as_bytes()))?;
        Ok(matches!(stmt.next(), Ok(sqlite::State::Row)))
    }

    fn parse_file(&self, path: &Path) -> anyhow::Result<Vec<Chunk>> {
        // Extract text based on extension
        // For PDF: pdf-extract, for docx: docx-rs, etc.
        // Chunk by headings, ~400 tokens, 10% overlap
        unimplemented!()
    }

    fn delete_old_chunks(&self, tx: &sqlite::Transaction, path: &Path) -> anyhow::Result<()> {
        tx.execute("DELETE FROM chunks WHERE path=?", [path.to_string_lossy().as_ref()])?;
        Ok(())
    }

    fn insert_chunk(&self, tx: &sqlite::Transaction, chunk: &Chunk, embedding: &[f32]) -> anyhow::Result<()> {
        let vec_blob = crate::search::tier1::serialize_f32_vec(embedding);
        tx.execute(
            "INSERT INTO chunks (path, page, line_start, line_end, content, content_hash, mtime, size, metadata)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                &chunk.path, chunk.page as i64, chunk.line_start as i64, chunk.line_end as i64,
                &chunk.content, chunk.content_hash.as_bytes(), chunk.mtime, chunk.size as i64, &chunk.metadata
            )
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO chunks_vec (id, embedding) VALUES (?, ?)", (id, vec_blob.as_slice()))?;
        Ok(())
    }

    fn watch(&self, _roots: &[PathBuf]) -> anyhow::Result<()> {
        // notify crate for file system events
        unimplemented!()
    }
}

struct Chunk {
    path: String,
    page: usize,
    line_start: usize,
    line_end: usize,
    content: String,
    content_hash: blake3::Hash,
    mtime: i64,
    size: usize,
    metadata: String,
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test indexer_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/core/index/indexer.rs tests/core/indexer_test.rs
git commit -m "feat: Incremental indexer with blake3 hashing"
```

---

### Task 9: CLI Commands

**Files:**
- Modify: `src/cli/mod.rs`
- Create: `src/cli/search.rs`
- Create: `src/cli/answer.rs`
- Create: `src/cli/index.rs`
- Create: `src/cli/models.rs`
- Create: `src/cli/doctor.rs`
- Test: `tests/cli/test.rs`

**Interfaces:**
- Consumes: `SearchPipeline`, `Indexer`, `ModelManager`
- Produces: CLI subcommands with `--json` support, exit codes

- [ ] **Step 1: Write failing test**

```rust
// tests/cli/test.rs
use std::process::Command;

#[test]
fn test_cli_search_json() {
    let output = Command::new("cargo")
        .args(["run", "--", "search", "warranty", "--json"])
        .output().unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.is_array());
}

#[test]
fn test_cli_exit_codes() {
    let output = Command::new("cargo")
        .args(["run", "--", "search", "nonexistentqueryxyz"])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(1)); // Not found
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test cli_test -- --nocapture
```

- [ ] **Step 3: Implement CLI modules**

```rust
// src/cli/mod.rs
pub mod search;
pub mod answer;
pub mod index;
pub mod models;
pub mod doctor;

use clap::{Args, Subcommand};
use anyhow::Result;

#[derive(Args)]
pub struct SearchArgs {
    pub query: String,
    #[arg(long, short)]
    pub root: Vec<String>,
    #[arg(long, short, default_value = "10")]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
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

pub async fn run_search(args: SearchArgs) -> Result<()> { search::run(args).await }
pub async fn run_answer(args: AnswerArgs) -> Result<()> { answer::run(args).await }
pub async fn run_index(args: IndexArgs) -> Result<()> { index::run(args).await }
pub async fn run_models(args: ModelsArgs) -> Result<()> { models::run(args).await }
pub async fn run_doctor() -> Result<()> { doctor::run().await }

// src/cli/search.rs
use crate::core::search::SearchPipeline;
use crate::core::models::ModelManager;
use crate::core::config::SearchConfig;

pub async fn run(args: SearchArgs) -> anyhow::Result<()> {
    let manager = ModelManager::new();
    let embedder = manager.get_embedder().await?;
    let pipeline = SearchPipeline::new(embedder, None, SearchConfig::default());
    
    let roots: Vec<_> = args.root.iter().map(std::path::PathBuf::from).collect();
    let hits = pipeline.search(&args.query, &roots)?;
    
    if args.json {
        println!("{}", serde_json::to_string_pretty(&hits)?);
    } else {
        for hit in hits.iter().take(args.limit) {
            println!("{} — {} p.{}", hit.snippet, hit.path, hit.page);
        }
    }
    std::process::exit(if hits.is_empty() { 1 } else { 0 });
}

// src/cli/answer.rs — similar, uses answer_documents logic

// src/cli/models.rs
pub async fn run(args: ModelsArgs) -> anyhow::Result<()> {
    let manager = ModelManager::new();
    match args.command {
        ModelsCommand::List => {
            for m in manager.list() {
                println!("{}  {}  {}  {}", 
                    if m.installed { "✓" } else { " " },
                    m.id, m.size_mb, m.name);
            }
        }
        ModelsCommand::Install { model } => {
            let path = manager.ensure(&model).await?;
            println!("Installed {} at {:?}", model, path);
        }
        ModelsCommand::Remove { model } => {
            let path = crate::paths::Paths::new().model_cache_dir().join(format!("{}.onnx", model));
            std::fs::remove_file(path)?;
            println!("Removed {}", model);
        }
    }
    Ok(())
}

// src/cli/doctor.rs
pub async fn run() -> anyhow::Result<()> {
    println!("=== Paper Doctor ===");
    // Check DB
    // Check models
    // Check OS brain availability
    // Check index stats
    Ok(())
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test cli_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/cli/ tests/cli/
git commit -m "feat: CLI commands (search, answer, index, models, doctor)"
```

---

### Task 10: MCP Server (stdio, 4 Tools)

**Files:**
- Modify: `src/mcp/mod.rs`
- Create: `src/mcp/server.rs`
- Create: `src/mcp/tools.rs`
- Test: `tests/mcp/test.rs`

**Interfaces:**
- Consumes: `SearchPipeline`
- Produces: `paper serve --mcp` runs stdio MCP server
- 4 tools: search_documents, read_source, list_sources, answer_documents

- [ ] **Step 1: Write failing test**

```rust
// tests/mcp/test.rs
use rmcp::Client;
use serde_json::json;

#[tokio::test]
async fn test_mcp_search_documents() {
    let server = tokio::process::Command::new("cargo")
        .args(["run", "--", "serve", "--mcp", "--root", "/tmp/test"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn().unwrap();
    
    let client = Client::new(server.stdin.unwrap(), server.stdout.unwrap()).await.unwrap();
    let result = client.call_tool("search_documents", json!({"query": "warranty"})).await.unwrap();
    assert!(result.is_array());
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test mcp_test -- --nocapture
```

- [ ] **Step 3: Implement MCP server**

```rust
// src/mcp/mod.rs
pub mod server;
pub mod tools;

use clap::Args;
use anyhow::Result;

#[derive(Args)]
pub struct ServeArgs {
    #[arg(long)]
    pub mcp: bool,
    #[arg(long, short)]
    pub root: Vec<String>,
}

pub async fn run_serve(args: ServeArgs) -> Result<()> {
    if args.mcp {
        server::run_mcp(args.root).await
    } else {
        anyhow::bail!("Only --mcp mode supported")
    }
}

// src/mcp/server.rs
use rmcp::{ServerHandler, tool, ToolResult, transport::stdio};
use crate::core::search::SearchPipeline;
use crate::core::models::ModelManager;
use crate::core::config::SearchConfig;

pub async fn run_mcp(roots: Vec<String>) -> anyhow::Result<()> {
    let manager = ModelManager::new();
    let embedder = manager.get_embedder().await?;
    let reranker = if manager.is_installed("bge-reranker-v2-m3") {
        Some(manager.get_reranker().await?)
    } else { None };
    
    let pipeline = SearchPipeline::new(embedder, reranker, SearchConfig::default());
    let server = PaperMcpServer { 
        pipeline: std::sync::Arc::new(pipeline),
        roots: roots.into_iter().map(std::path::PathBuf::from).collect(),
    };
    
    let transport = stdio::StdioTransport::new();
    ServerHandler::serve(server, transport).await?;
    Ok(())
}

#[derive(Clone)]
pub struct PaperMcpServer {
    pipeline: std::sync::Arc<SearchPipeline>,
    roots: Vec<std::path::PathBuf>,
}

#[tool(tool_name = "search_documents")]
impl PaperMcpServer {
    async fn search_documents(&self, query: String, roots: Option<Vec<String>>, limit: Option<usize>) -> ToolResult {
        let roots = roots.unwrap_or_else(|| self.roots.clone()).into_iter().map(std::path::PathBuf::from).collect();
        let hits = self.pipeline.search(&query, &roots)?;
        ToolResult::success(serde_json::to_value(hits.into_iter().take(limit.unwrap_or(10)).collect::<Vec<_>>())?)
    }
}

#[tool(tool_name = "read_source")]
impl PaperMcpServer {
    async fn read_source(&self, source_id: i64) -> ToolResult {
        // Validate against allowlist, return full text
        ToolResult::success(serde_json::json!({}))
    }
}

#[tool(tool_name = "list_sources")]
impl PaperMcpServer {
    async fn list_sources(&self, roots: Option<Vec<String>>) -> ToolResult {
        ToolResult::success(serde_json::json!({}))
    }
}

#[tool(tool_name = "answer_documents")]
impl PaperMcpServer {
    async fn answer_documents(&self, question: String, roots: Option<Vec<String>>, polish: Option<bool>) -> ToolResult {
        // Extractive answer + citations
        ToolResult::success(serde_json::json!({}))
    }
}
```

- [ ] **Step 4: Run test — expect PASS**

```bash
cargo test mcp_test -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add src/mcp/ tests/mcp/
git commit -m "feat: MCP stdio server with 4 tools"
```

---

### Task 11: Platform Tier 0 (Windows Search / Linux FTS5)

**Files:**
- Create: `src/core/platform/windows.rs`
- Create: `src/core/platform/linux.rs`
- Create: `src/core/platform/macos.rs`
- Test: `tests/core/platform_test.rs`

**Interfaces:**
- Consumes: `tier0_search` calls platform-specific impl
- Produces: `tier0_keyword(query, roots, limit) -> Result<Vec<RawHit>>`

- [ ] **Step 1: Write failing test** — platform-specific, skip on CI

- [ ] **Step 2: Implement platform modules**

```rust
// src/core/platform/macos.rs — already in tier0.rs

// src/core/platform/windows.rs
pub fn tier0_windows_search(query: &str, roots: &[PathBuf], limit: usize) -> anyhow::Result<Vec<RawHit>> {
    // Windows Search via Windows.Storage.Search or WDS
    // Fallback to FTS5 if not available
    tier0_fts5(query, roots, limit)
}

// src/core/platform/linux.rs
pub fn tier0_linux_search(query: &str, roots: &[PathBuf], limit: usize) -> anyhow::Result<Vec<RawHit>> {
    // No system index; use FTS5
    tier0_fts5(query, roots, limit)
}
```

- [ ] **Step 3: Wire into tier0.rs with `cfg(target_os)`**

- [ ] **Step 4: Commit**

```bash
git add src/core/platform/
git commit -m "feat: Platform Tier 0 (macOS mdfind, Windows, Linux FTS5 fallback)"
```

---

### Task 12: End-to-End Integration Test

**Files:**
- Create: `tests/integration/e2e_test.rs`

**Interfaces:**
- Full flow: index → search → answer → MCP call

- [ ] **Step 1: Write test**

```rust
// tests/integration/e2e_test.rs
use std::process::Command;
use tempfile::tempdir;
use std::fs;

#[test]
fn test_full_flow() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("warranty.txt");
    fs::write(&file, "The fridge warranty covers 24 months from 2025-01-15. It expires on 2027-01-15.").unwrap();
    
    // Index
    let output = Command::new("./target/release/paper")
        .args(["index", dir.path().to_str().unwrap()])
        .output().unwrap();
    assert!(output.status.success());
    
    // Search
    let output = Command::new("./target/release/paper")
        .args(["search", "fridge warranty", "--json"])
        .output().unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.as_array().unwrap().len() > 0);
    
    // Answer
    let output = Command::new("./target/release/paper")
        .args(["answer", "when does the fridge warranty expire?"])
        .output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2027-01-15") || stdout.contains("24 months"));
}
```

- [ ] **Step 2: Run test — expect PASS**

```bash
cargo build --release
cargo test e2e_test -- --nocapture
```

- [ ] **Step 3: Commit**

```bash
git add tests/integration/
git commit -m "test: End-to-end integration test"
```

---

### Task 13: Release Artifacts & Installers

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `packaging/homebrew/paper.rb`
- Create: `packaging/scoop/paper.json`

**Interfaces:**
- Produces: signed binaries, checksums, installers

- [ ] **Step 1: Implement GitHub Actions release workflow**

```yaml
# .github/workflows/release.yml
name: Release
on:
  push:
    tags: ['v*']
jobs:
  build:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [macos-latest, windows-latest, ubuntu-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Build
        run: cargo build --release
      - name: Package
        run: |
          # Create tarball/zip with binary + README
      - name: Upload
        uses: actions/upload-artifact@v4
  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
      - name: Sign
        run: cosign sign-blob ...
      - name: Create Release
        uses: softprops/action-gh-release@v1
```

- [ ] **Step 2: Create Homebrew formula**

```ruby
# packaging/homebrew/paper.rb
class Paper < Formula
  desc "Local document finder with citations"
  homepage "https://github.com/yourname/paper"
  version "0.1.0"
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/yourname/paper/releases/download/v#{version}/paper-aarch64-apple-darwin.tar.gz"
      sha256 "..."
    else
      url "https://github.com/yourname/paper/releases/download/v#{version}/paper-x86_64-apple-darwin.tar.gz"
      sha256 "..."
    end
  end
  def install
    bin.install "paper"
  end
end
```

- [ ] **Step 3: Create Scoop manifest**

```json
// packaging/scoop/paper.json
{
  "version": "0.1.0",
  "description": "Local document finder with citations",
  "homepage": "https://github.com/yourname/paper",
  "license": "MIT",
  "architecture": {
    "64bit": {
      "url": "https://github.com/yourname/paper/releases/download/v0.1.0/paper-x86_64-pc-windows-msvc.zip",
      "hash": "..."
    }
  },
  "bin": "paper.exe"
}
```

- [ ] **Step 4: Commit**

```bash
git add .github/ packaging/
git commit -m "chore: Release workflow and installers"
```

---

## Summary

| Task | Component | Est. Lines |
|---|---|---|
| 1 | Workspace skeleton | 50 |
| 2 | Config & paths | 80 |
| 3 | SQLite schema | 100 |
| 4 | Models (EmbeddingGemma + reranker) | 250 |
| 5 | Tier 0 (mdfind/FTS5) | 120 |
| 6 | Tier 1 (sqlite-vec) | 80 |
| 7 | Pipeline + RRF | 150 |
| 8 | Indexer (incremental) | 200 |
| 9 | CLI commands | 180 |
| 10 | MCP server (4 tools) | 150 |
| 11 | Platform Tier 0 | 60 |
| 12 | E2E test | 80 |
| 13 | Release artifacts | 60 |
| **Total** | | **~1,620** |

Each task ends with a passing test and a commit. The final binary is `paper` with all subcommands.