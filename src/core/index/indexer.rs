use super::connection::Connection;
use super::schema::init_db;
use crate::core::search::serialize_f32_vec;
use crate::core::search::SharedEmbedder;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Formats we can extract text from without any external tool.
pub const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "rst", "csv", "tsv", "json", "yaml", "yml", "toml", "xml", "html",
    "htm", "log", "tex",
];

/// Files larger than this are skipped rather than loaded into memory.
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IndexStats {
    pub scanned: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub chunks: usize,
}

impl IndexStats {
    pub fn merge(&mut self, other: IndexStats) {
        self.scanned += other.scanned;
        self.indexed += other.indexed;
        self.unchanged += other.unchanged;
        self.skipped += other.skipped;
        self.chunks += other.chunks;
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub page: i64,
    pub line_start: i64,
    pub line_end: i64,
    pub content: String,
}

pub struct Indexer {
    conn: Connection,
    embedder: Option<SharedEmbedder>,
    max_chunk_words: usize,
    overlap_words: usize,
}

impl Indexer {
    pub fn new(conn: Connection, embedder: Option<SharedEmbedder>) -> Result<Self> {
        init_db(&conn)?;
        Ok(Self {
            conn,
            embedder,
            max_chunk_words: 400,
            overlap_words: 40,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn index_roots(&self, roots: &[PathBuf]) -> Result<IndexStats> {
        let mut stats = IndexStats::default();
        for root in roots {
            if root.is_file() {
                stats.merge(self.index_file(root)?);
                continue;
            }
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| !is_hidden(e.file_name()))
            {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("walk error: {e}");
                        stats.skipped += 1;
                        continue;
                    }
                };
                if entry.file_type().is_file() {
                    stats.merge(self.index_file(entry.path())?);
                }
            }
        }
        Ok(stats)
    }

    pub fn index_file(&self, path: &Path) -> Result<IndexStats> {
        let mut stats = IndexStats {
            scanned: 1,
            ..Default::default()
        };
        if !is_indexable(path) {
            stats.skipped = 1;
            return Ok(stats);
        }
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > MAX_FILE_BYTES {
            stats.skipped = 1;
            return Ok(stats);
        }

        let path_string = path.to_string_lossy().to_string();
        let mtime = metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let size = metadata.len() as i64;

        // Cheap pre-check: identical path + mtime + size means nothing to do.
        let unchanged: Option<i64> = self
            .conn
            .inner()
            .query_row(
                "SELECT 1 FROM chunks WHERE path = ? AND mtime = ? AND size = ? LIMIT 1",
                rusqlite::params![path_string, mtime, size],
                |row| row.get(0),
            )
            .optional()?;
        if unchanged.is_some() {
            stats.unchanged = 1;
            return Ok(stats);
        }

        let bytes = std::fs::read(path)?;
        let hash = blake3::hash(&bytes);
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                // Not UTF-8 text: nothing we can cite line numbers for.
                stats.skipped = 1;
                return Ok(stats);
            }
        };
        let chunks = chunk_text(&text, self.max_chunk_words, self.overlap_words);

        let tx = self.conn.inner().unchecked_transaction()?;
        tx.execute(
            "DELETE FROM chunks_vec WHERE rowid IN (SELECT id FROM chunks WHERE path = ?)",
            [&path_string],
        )?;
        tx.execute("DELETE FROM chunks WHERE path = ?", [&path_string])?;

        let mut inserted = 0usize;
        for chunk in &chunks {
            tx.execute(
                "INSERT INTO chunks (path, page, line_start, line_end, content, content_hash, mtime, size, metadata)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, '{}')",
                rusqlite::params![
                    path_string,
                    chunk.page,
                    chunk.line_start,
                    chunk.line_end,
                    chunk.content,
                    hash.as_bytes().as_slice(),
                    mtime,
                    size,
                ],
            )?;
            let id = tx.last_insert_rowid();
            if let Some(embedder) = &self.embedder {
                let vector = embedder.embed(&chunk.content)?;
                tx.execute(
                    "INSERT INTO chunks_vec (rowid, embedding) VALUES (?, ?)",
                    rusqlite::params![id, serialize_f32_vec(&vector)],
                )?;
            }
            inserted += 1;
        }
        tx.commit()?;

        stats.indexed = 1;
        stats.chunks = inserted;
        Ok(stats)
    }

    /// Drop every chunk belonging to files that no longer exist on disk.
    pub fn prune_missing(&self) -> Result<usize> {
        let mut stmt = self.conn.inner().prepare("SELECT DISTINCT path FROM chunks")?;
        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let mut removed = 0;
        for path in paths {
            if !Path::new(&path).exists() {
                self.conn.inner().execute(
                    "DELETE FROM chunks_vec WHERE rowid IN (SELECT id FROM chunks WHERE path = ?)",
                    [&path],
                )?;
                self.conn
                    .inner()
                    .execute("DELETE FROM chunks WHERE path = ?", [&path])?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

use rusqlite::OptionalExtension;

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

pub fn is_indexable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| TEXT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Split text into overlapping chunks while preserving 1-based line numbers so
/// every hit can be cited as `path:line_start-line_end`.
pub fn chunk_text(text: &str, max_words: usize, overlap_words: usize) -> Vec<Chunk> {
    let max_words = max_words.max(1);
    let overlap = overlap_words.min(max_words - 1);
    let lines: Vec<&str> = text.lines().collect();
    let mut chunks = Vec::new();

    let mut current: Vec<&str> = Vec::new();
    let mut current_words = 0usize;
    let mut start_line = 1usize;

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let words = line.split_whitespace().count();
        if current_words > 0 && current_words + words > max_words {
            chunks.push(Chunk {
                page: 0,
                line_start: start_line as i64,
                line_end: (line_no - 1) as i64,
                content: current.join("\n"),
            });
            // Overlap: keep the tail of the previous chunk for context.
            let mut kept: Vec<&str> = Vec::new();
            let mut kept_words = 0usize;
            for prev in current.iter().rev() {
                let w = prev.split_whitespace().count();
                if kept_words + w > overlap {
                    break;
                }
                kept.insert(0, prev);
                kept_words += w;
            }
            start_line = line_no - kept.len();
            current = kept;
            current_words = kept_words;
        }
        if current.is_empty() {
            start_line = line_no;
        }
        current.push(line);
        current_words += words;
    }
    if current.iter().any(|l| !l.trim().is_empty()) {
        chunks.push(Chunk {
            page: 0,
            line_start: start_line as i64,
            line_end: lines.len().max(1) as i64,
            content: current.join("\n"),
        });
    }
    chunks
}

/// Convenience used by the CLI: open the standard database and index.
pub fn index_with_defaults(
    db_path: &Path,
    roots: &[PathBuf],
    embedder: Option<SharedEmbedder>,
) -> Result<IndexStats> {
    let conn = Connection::open(db_path).context("opening index database")?;
    Indexer::new(conn, embedder)?.index_roots(roots)
}
