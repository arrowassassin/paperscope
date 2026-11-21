use super::connection::Connection;
use super::schema::init_db;
use crate::core::extract;
use crate::core::fields;
use crate::core::search::serialize_f32_vec;
use crate::core::search::SharedEmbedder;
use crate::core::text::{fts_segment, same_tokens};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IndexStats {
    pub scanned: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub chunks: usize,
    /// Explicit extraction problems (a PDF page with no text layer, an
    /// unreadable worksheet). Reported, never silently dropped.
    pub warnings: usize,
}

impl IndexStats {
    pub fn merge(&mut self, other: IndexStats) {
        self.scanned += other.scanned;
        self.indexed += other.indexed;
        self.unchanged += other.unchanged;
        self.skipped += other.skipped;
        self.chunks += other.chunks;
        self.warnings += other.warnings;
    }
}

/// A citable unit of text. Every field is copied from the source: the anchor
/// and section come from the document's own structure, never from inference.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub page: i64,
    pub line_start: i64,
    pub line_end: i64,
    /// Format-specific anchor (`Sheet1!A1:H1`, `slide 2`, `para 4-9`).
    pub anchor: Option<String>,
    /// Declared section path (`Warranty / Coverage`).
    pub section: Option<String>,
    pub content: String,
}

/// Bump when extraction, chunking or the FTS projection changes meaning. A
/// mismatch invalidates every stored chunk, because the old text can no longer
/// be trusted to match what the current extractor would produce.
const INDEXER_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+extract3+chunk1");

pub struct Indexer {
    conn: Connection,
    embedder: Option<SharedEmbedder>,
    max_chunk_words: usize,
    overlap_words: usize,
    ocr_enabled: bool,
}

impl Indexer {
    pub fn new(conn: Connection, embedder: Option<SharedEmbedder>) -> Result<Self> {
        Self::with_ocr(conn, embedder, false)
    }

    pub fn with_ocr(
        conn: Connection,
        embedder: Option<SharedEmbedder>,
        ocr_enabled: bool,
    ) -> Result<Self> {
        init_db(&conn)?;
        invalidate_on_upgrade(&conn)?;
        Ok(Self {
            conn,
            embedder,
            max_chunk_words: 400,
            overlap_words: 40,
            ocr_enabled,
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
                // `follow_links(false)` means symlinks are neither files nor
                // directories here, so they are never followed.
                if entry.file_type().is_file() {
                    // One unreadable file must not abort a whole directory.
                    match self.index_file(entry.path()) {
                        Ok(file_stats) => stats.merge(file_stats),
                        Err(err) => {
                            tracing::warn!("{}: {err}", entry.path().display());
                            stats.skipped += 1;
                        }
                    }
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
        let path_string = path.to_string_lossy().to_string();
        if !extract::is_supported(&path_string) {
            stats.skipped = 1;
            return Ok(stats);
        }
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            stats.skipped = 1;
            return Ok(stats);
        }
        // Respect each extractor's own size cap instead of a single global cap,
        // so a 100 MB PDF isn't skipped just because the default is 64 MB (C3).
        // Find every extractor that claims this extension and use the largest cap.
        let mut max_allowed: u64 = 0;
        for extractor in extract::extractors() {
            if extractor
                .extensions()
                .contains(&extract::extension_of(&path_string).as_str())
            {
                max_allowed = max_allowed.max(extractor.capabilities().max_file_bytes);
            }
        }
        if max_allowed == 0 || metadata.len() > max_allowed {
            stats.skipped = 1;
            return Ok(stats);
        }

        let mtime = metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let size = metadata.len() as i64;

        // Cheap pre-check: identical source path + mtime + size means nothing to
        // do. Archive members share the outer archive's mtime/size.
        let unchanged: Option<i64> = self
            .conn
            .inner()
            .query_row(
                "SELECT 1 FROM chunks WHERE source_path = ? AND mtime = ? AND size = ? LIMIT 1",
                rusqlite::params![path_string, mtime, size],
                |row| row.get(0),
            )
            .optional()?;
        if unchanged.is_some() {
            stats.unchanged = 1;
            return Ok(stats);
        }

        let bytes = std::fs::read(path).with_context(|| format!("reading {path_string}"))?;
        let hash = blake3::hash(&bytes);

        // A new mtime or size does not always mean new content: `touch -r`,
        // restores from backup and rsync all preserve bytes. If the content hash
        // still matches, only the recorded mtime/size change and no parsing,
        // chunking or embedding work is repeated.
        let stored_hash: Option<Vec<u8>> = self
            .conn
            .inner()
            .query_row(
                "SELECT content_hash FROM chunks WHERE source_path = ? LIMIT 1",
                [&path_string],
                |row| row.get(0),
            )
            .optional()?;
        if stored_hash.as_deref() == Some(hash.as_bytes().as_slice()) {
            self.conn.inner().execute(
                "UPDATE chunks SET mtime = ?, size = ? WHERE source_path = ?",
                rusqlite::params![mtime, size, path_string],
            )?;
            stats.unchanged = 1;
            return Ok(stats);
        }

        let docs = match extract::extract_named(&path_string, &bytes, self.ocr_enabled)? {
            Some(docs) => docs,
            None => {
                stats.skipped = 1;
                return Ok(stats);
            }
        };

        let tx = self.conn.inner().unchecked_transaction()?;
        tx.execute(
            "DELETE FROM chunks_vec WHERE rowid IN (SELECT id FROM chunks WHERE source_path = ?)",
            [&path_string],
        )?;
        tx.execute("DELETE FROM chunks WHERE source_path = ?", [&path_string])?;

        let mut inserted = 0usize;
        let mut warnings = 0usize;
        for doc in &docs {
            for warning in &doc.warnings {
                tracing::warn!("{path_string}: {warning}");
                warnings += 1;
            }
            let chunks = chunk_blocks(&doc.blocks, self.max_chunk_words, self.overlap_words);
            for chunk in &chunks {
                // `fts_text` / `fts_section` hold only text that segmentation
                // actually changed. Latin text segments to itself, and storing
                // that copy doubled the size of most indexes; the triggers fall
                // back to `content` and `section` instead.
                //
                // Compare whitespace-separated tokens, not bytes: segmentation
                // normalises whitespace, and FTS5 tokenises newlines and spaces
                // identically, so `a\nb` and `a b` produce the same index and
                // must not be stored twice.
                let mut fts_text = fts_segment(&chunk.content);
                if same_tokens(&fts_text, &chunk.content) {
                    fts_text.clear();
                }
                let section = chunk.section.as_deref().unwrap_or("");
                let mut fts_section = fts_segment(section);
                if same_tokens(&fts_section, section) {
                    fts_section.clear();
                }
                let metadata = fields::analyse(&chunk.content);
                tx.execute(
                    "INSERT INTO chunks
                       (path, source_path, page, line_start, line_end, anchor, section, content,
                        fts_text, fts_section, content_hash, mtime, size, metadata)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params![
                        doc.path,
                        path_string,
                        chunk.page,
                        chunk.line_start,
                        chunk.line_end,
                        chunk.anchor,
                        chunk.section,
                        chunk.content,
                        fts_text,
                        fts_section,
                        hash.as_bytes().as_slice(),
                        mtime,
                        size,
                        metadata,
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
        }
        tx.commit()?;

        stats.warnings = warnings;
        if inserted == 0 {
            stats.skipped = 1;
        } else {
            stats.indexed = 1;
            stats.chunks = inserted;
        }
        Ok(stats)
    }

    /// Re-index an already-known file (used by watch mode).
    pub fn reindex_file(&self, path: &Path) -> Result<IndexStats> {
        self.index_file(path)
    }

    /// Drop every chunk belonging to files that no longer exist on disk.
    pub fn prune_missing(&self) -> Result<usize> {
        let mut stmt = self
            .conn
            .inner()
            .prepare("SELECT DISTINCT source_path FROM chunks")?;
        let paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        drop(stmt);

        // One `stat` per indexed document is unavoidable, but the deletes are
        // not: batching them into a single transaction turns N fsyncs into one.
        let missing: Vec<String> = paths
            .into_iter()
            .filter(|path| !path.is_empty() && !Path::new(path).exists())
            .collect();
        if missing.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.inner().unchecked_transaction()?;
        {
            let mut del_vec = tx.prepare(
                "DELETE FROM chunks_vec WHERE rowid IN
                     (SELECT id FROM chunks WHERE source_path = ?1)",
            )?;
            let mut del_chunks = tx.prepare("DELETE FROM chunks WHERE source_path = ?1")?;
            for path in &missing {
                del_vec.execute([path])?;
                del_chunks.execute([path])?;
            }
        }
        tx.commit()?;
        let removed = missing.len();
        // Checkpoint and truncate the WAL so deleted pages are reclaimed and
        // the -wal/-shm files don't grow unbounded.
        self.conn
            .inner()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(removed)
    }
}

use rusqlite::OptionalExtension;

/// Wipe stored chunks when the extractor/chunker version changes.
///
/// Without this, upgrading Paper would leave chunks produced by an older parser
/// in place, and their anchors could point at text the current parser no longer
/// produces. A full reindex is the honest outcome; it happens once per upgrade.
fn invalidate_on_upgrade(conn: &Connection) -> Result<()> {
    let stored: Option<String> = conn
        .inner()
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'indexer_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if stored.as_deref() == Some(INDEXER_VERSION) {
        return Ok(());
    }
    let tx = conn.inner().unchecked_transaction()?;
    tx.execute("DELETE FROM chunks_vec", [])?;
    tx.execute("DELETE FROM chunks", [])?;
    tx.execute(
        "INSERT INTO index_meta (key, value) VALUES ('indexer_version', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [INDEXER_VERSION],
    )?;
    tx.commit()?;
    Ok(())
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

pub fn is_indexable(path: &Path) -> bool {
    extract::is_supported(&path.to_string_lossy())
}

/// Split text into overlapping chunks while preserving 1-based line numbers so
/// every hit can be cited as `path:line_start-line_end`.
///
/// Markdown headings start a new chunk, which keeps sections from being cut in
/// half and gives the retriever cleaner units.
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
        let heading = is_markdown_heading(line);
        if !current.is_empty() && (current_words + words > max_words || heading) {
            chunks.push(Chunk {
                page: 0,
                line_start: start_line as i64,
                line_end: (line_no - 1) as i64,
                anchor: None,
                section: None,
                content: current.join("\n"),
            });
            // Overlap: keep the tail of the previous chunk for context. A
            // heading is a hard boundary, so it never inherits the old tail.
            let mut kept: Vec<&str> = Vec::new();
            let mut kept_words = 0usize;
            if !heading {
                for prev in current.iter().rev() {
                    let w = prev.split_whitespace().count();
                    if kept_words + w > overlap {
                        break;
                    }
                    kept.insert(0, prev);
                    kept_words += w;
                }
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
            anchor: None,
            section: None,
            content: current.join("\n"),
        });
    }
    chunks
}

fn is_markdown_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes) && trimmed.chars().nth(hashes) == Some(' ')
}

/// Turn extracted blocks into citable chunks. Blocks are never merged across
/// anchors or sections, so a citation can always name the exact page/cell/slide
/// and the section it belongs to.
pub fn chunk_blocks(
    blocks: &[extract::Block],
    max_words: usize,
    overlap_words: usize,
) -> Vec<Chunk> {
    let mut out = Vec::new();
    for block in blocks {
        for chunk in chunk_text(&block.text, max_words, overlap_words) {
            let (line_start, line_end) = if block.line_start > 0 {
                (
                    block.line_start + chunk.line_start - 1,
                    block.line_start + chunk.line_end - 1,
                )
            } else {
                (0, 0)
            };
            out.push(Chunk {
                page: block.page,
                line_start,
                line_end,
                anchor: block.anchor.clone(),
                section: block.section.clone(),
                content: chunk.content,
            });
        }
    }
    out
}

/// Convenience used by the CLI: atomically index into a temp database, then
/// rename into place. This guarantees that a crash mid-index never leaves a
/// corrupted `chunks.db`.
pub fn index_with_defaults(
    db_path: &Path,
    roots: &[PathBuf],
    embedder: Option<SharedEmbedder>,
) -> Result<IndexStats> {
    index_with_defaults_atomic(db_path, roots, embedder)
}

/// Internal: perform a full index into `db_path.tmp` then atomically replace
/// `db_path`. On cross-device rename failure, fall back to copy + remove.
fn index_with_defaults_atomic(
    db_path: &Path,
    roots: &[PathBuf],
    embedder: Option<SharedEmbedder>,
) -> Result<IndexStats> {
    let tmp_path = db_path.with_extension("db.tmp");

    // Remove any stale temp file from a previous interrupted run.
    let _ = fs::remove_file(&tmp_path);

    // Open the temp database with creation-time pragmas (page_size, etc.).
    let conn = Connection::open_or_create(&tmp_path).context("opening temp index database")?;
    let stats = Indexer::new(conn, embedder)?.index_roots(roots)?;

    // Commit is implicit in Indexer::index_roots (each file is its own tx).
    // Now atomically promote the temp file.
    atomic_replace(&tmp_path, db_path)?;

    Ok(stats)
}

/// Atomically replace `dst` with `src` using rename, with cross-device fallback.
fn atomic_replace(src: &Path, dst: &Path) -> Result<()> {
    // Try the fast path: same filesystem, atomic rename.
    match fs::rename(src, dst) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            // Fallback: copy + remove. Not atomic across devices, but the best
            // we can do. The destination is only visible after copy completes.
            fs::copy(src, dst).with_context(|| {
                format!(
                    "copying {} -> {} (cross-device)",
                    src.display(),
                    dst.display()
                )
            })?;
            fs::remove_file(src)
                .with_context(|| format!("removing temp file {}", src.display()))?;
        }
        Err(e) => {
            return Err(e)
                .with_context(|| format!("renaming {} -> {}", src.display(), dst.display()))
        }
    }
    Ok(())
}
