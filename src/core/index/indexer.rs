use super::connection::Connection;
use super::schema::init_db;
use crate::core::extract;
use crate::core::fields;
use crate::core::search::serialize_f32_vec;
use crate::core::search::SharedEmbedder;
use crate::core::text::{fts_segment, same_tokens};
use anyhow::{Context, Result};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IndexStats {
    pub scanned: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub chunks: usize,
    /// Explicit extraction problems (a PDF page with no text layer, an
    /// unreadable worksheet). Reported, never silently dropped.
    pub warnings: usize,
    /// Entries this pass could not read for lack of permission — a macOS TCC
    /// grant that was never given, a POSIX mode, a Windows ACL. Counted once per
    /// pass and reported as one line per root, never one warning per file.
    #[serde(default)]
    pub denied: usize,
    /// Entries skipped before touching the filesystem because the previous pass
    /// already found them unreadable and the backoff window has not elapsed.
    #[serde(default)]
    pub backed_off: usize,
}

impl IndexStats {
    pub fn merge(&mut self, other: IndexStats) {
        self.scanned += other.scanned;
        self.indexed += other.indexed;
        self.unchanged += other.unchanged;
        self.skipped += other.skipped;
        self.chunks += other.chunks;
        self.warnings += other.warnings;
        self.denied += other.denied;
        self.backed_off += other.backed_off;
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
    /// Indexer with the default OCR policy.
    ///
    /// OCR is on when its weights are present, so the daemon and the MCP tools
    /// pick it up without each caller having to remember the flag.
    pub fn new(conn: Connection, embedder: Option<SharedEmbedder>) -> Result<Self> {
        Self::with_ocr(conn, embedder, ocr_by_default())
    }

    /// Whether extraction in this indexer may run OCR.
    pub fn ocr_enabled(&self) -> bool {
        self.ocr_enabled
    }

    pub fn with_ocr(
        conn: Connection,
        embedder: Option<SharedEmbedder>,
        ocr_enabled: bool,
    ) -> Result<Self> {
        init_db(&conn)?;
        invalidate_on_upgrade(&conn)?;
        // Record the embedding model used for this index, so we can detect
        // mismatches at search time.
        if let Some(embedder) = &embedder {
            Self::record_embedding_model(&conn, embedder.model_id())?;
        }
        Ok(Self {
            conn,
            embedder,
            max_chunk_words: 400,
            overlap_words: 40,
            ocr_enabled,
        })
    }

    /// Store the embedding model ID in index_meta. This is called when the
    /// indexer is created with an embedder, so the index knows which model
    /// produced its vectors.
    fn record_embedding_model(conn: &Connection, model_id: &str) -> Result<()> {
        conn.inner().execute(
            "INSERT INTO index_meta (key, value) VALUES ('embedding_model', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [model_id],
        )?;
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn index_roots(&self, roots: &[PathBuf]) -> Result<IndexStats> {
        let mut stats = IndexStats::default();
        // Paths a recent pass already failed to read, loaded once: the walk
        // consults this set instead of asking the database per file, and a
        // directory in it is pruned whole rather than walked and failed again.
        let denied: RefCell<HashSet<PathBuf>> =
            RefCell::new(self.recently_denied()?.into_iter().collect());
        // Directories pruned by the filter, counted here because `filter_entry`
        // cannot touch `stats` while the iterator holds it.
        let pruned_dirs = Cell::new(0usize);

        for root in roots {
            if root.is_file() {
                // A root that is itself unreadable gets the same treatment as an
                // unreadable file inside one: reported, remembered, not fatal.
                // Propagating here made `--file` (and `--root <file>`) exit 2
                // while the identical file inside a walked root was fine.
                match self.index_file(root) {
                    Ok(file_stats) => stats.merge(file_stats),
                    Err(err) if is_permission_denied(&err) => {
                        self.note_denied(root, &format!("{err:#}"));
                        denied.borrow_mut().insert(root.clone());
                        stats.denied += 1;
                        tracing::warn!("{}", denied_summary(root, 1));
                    }
                    Err(err) => return Err(err),
                }
                continue;
            }
            let mut root_denied = 0usize;
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                // Hidden entries are skipped; a directory already known to be
                // unreadable is pruned whole, while a denied *file* stays in the
                // walk so it can be counted as deferred rather than vanishing.
                .filter_entry(|e| {
                    if is_hidden(e.file_name()) {
                        return false;
                    }
                    if e.file_type().is_dir() && denied.borrow().contains(e.path()) {
                        pruned_dirs.set(pruned_dirs.get() + 1);
                        return false;
                    }
                    true
                })
            {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        // A directory we may not read is not a per-file problem:
                        // remember it, keep walking, and say it once at the end
                        // of the root. One line per denied entry is how a single
                        // revoked grant became 1,070 warnings per five minutes.
                        if is_walk_permission_denied(&e) {
                            if let Some(path) = e.path() {
                                self.note_denied(path, &format!("{e}"));
                                denied.borrow_mut().insert(path.to_path_buf());
                            }
                            root_denied += 1;
                        } else {
                            // NotFound included: a path that vanished mid-walk is
                            // not a permission problem, and telling the user to
                            // grant Full Disk Access for it would be a lie.
                            tracing::warn!("walk error: {e:#}");
                        }
                        stats.skipped += 1;
                        continue;
                    }
                };
                // `follow_links(false)` means symlinks are neither files nor
                // directories here, so they are never followed.
                if entry.file_type().is_file() {
                    if denied.borrow().contains(entry.path()) {
                        stats.backed_off += 1;
                        continue;
                    }
                    // One unreadable file must not abort a whole directory.
                    match self.index_file(entry.path()) {
                        Ok(file_stats) => stats.merge(file_stats),
                        Err(err) => {
                            if is_permission_denied(&err) {
                                self.note_denied(entry.path(), &format!("{err:#}"));
                                denied.borrow_mut().insert(entry.path().to_path_buf());
                                root_denied += 1;
                            } else {
                                tracing::warn!("{}: {err:#}", entry.path().display());
                            }
                            stats.skipped += 1;
                        }
                    }
                }
            }
            stats.denied += root_denied;
            if root_denied > 0 {
                tracing::warn!("{}", denied_summary(root, root_denied));
            }
        }
        stats.backed_off += pruned_dirs.get();
        // Let the cache expire on its own: a path whose permission was fixed is
        // tried again one backoff window after its last failure.
        if let Err(err) = self.expire_unreadable() {
            tracing::warn!("could not age out the unreadable cache: {err:#}");
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

    /// Paths a recent pass could not read, whose backoff window has not elapsed.
    fn recently_denied(&self) -> Result<Vec<PathBuf>> {
        let cutoff = now_unix() - DENIED_BACKOFF.as_secs() as i64;
        let mut stmt = self
            .conn
            .inner()
            .prepare("SELECT path FROM unreadable WHERE last_attempt > ?1")?;
        let rows = stmt.query_map([cutoff], |row| row.get::<_, String>(0))?;
        Ok(rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(PathBuf::from)
            .collect())
    }

    /// Remember that `path` could not be read, and when it was last tried.
    ///
    /// Bookkeeping must never fail an indexing run: a locked database here would
    /// turn "one file is unreadable" into "the whole pass failed", which is a
    /// worse outcome than losing the backoff for one pass.
    fn note_denied(&self, path: &Path, reason: &str) {
        if let Err(err) = self.remember_unreadable(path, reason) {
            tracing::warn!(
                "could not record {} as unreadable ({err:#}); it will be retried next pass",
                path.display()
            );
        }
    }

    fn remember_unreadable(&self, path: &Path, reason: &str) -> Result<()> {
        let now = now_unix();
        self.conn.inner().execute(
            "INSERT INTO unreadable (path, reason, first_seen, last_attempt)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(path) DO UPDATE SET
                 reason = excluded.reason,
                 last_attempt = excluded.last_attempt",
            rusqlite::params![path.to_string_lossy(), reason, now],
        )?;
        Ok(())
    }

    /// Drop entries whose backoff has elapsed, so a fixed permission is retried
    /// on the next pass instead of being excluded forever.
    fn expire_unreadable(&self) -> Result<()> {
        let cutoff = now_unix() - DENIED_BACKOFF.as_secs() as i64;
        self.conn
            .inner()
            .execute("DELETE FROM unreadable WHERE last_attempt <= ?1", [cutoff])?;
        Ok(())
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
        //
        // A path inside an unreadable directory answers `exists() == false` too,
        // so pruning on that alone deleted the indexed content of exactly the
        // roots this change exists to tolerate (revoked TCC grant → "1 removed"
        // → search returns nothing). Never prune what we could not read.
        let unreadable = self.recently_denied()?;
        let hidden_by_permission = |path: &str| {
            let path = Path::new(path);
            unreadable.iter().any(|denied| path.starts_with(denied))
        };
        let missing: Vec<String> = paths
            .into_iter()
            .filter(|path| {
                !path.is_empty() && !Path::new(path).exists() && !hidden_by_permission(path)
            })
            .collect();
        if missing.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.inner().unchecked_transaction()?;
        let mut removed = 0usize;
        {
            let mut del_vec = tx.prepare(
                "DELETE FROM chunks_vec WHERE rowid IN
                     (SELECT id FROM chunks WHERE source_path = ?1)",
            )?;
            let mut del_chunks = tx.prepare("DELETE FROM chunks WHERE source_path = ?1")?;
            for path in &missing {
                // Re-verify existence immediately before delete so a file
                // recreated between the check and the delete is not pruned
                // (TOCTOU, I8).
                if !path.is_empty() && !Path::new(path).exists() {
                    del_vec.execute([path])?;
                    del_chunks.execute([path])?;
                    removed += 1;
                }
            }
        }
        tx.commit()?;
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

/// How long an unreadable path is left alone before it is tried again. Long
/// enough that the five-minute tick does not turn one revoked grant into a
/// warning every pass; short enough that fixing the permission (or granting
/// Full Disk Access) takes effect without restarting anything.
const DENIED_BACKOFF: Duration = Duration::from_secs(10 * 60);

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whether an error chain contains the filesystem refusing access. `anyhow`
/// context wraps the kind behind a message, so the chain is walked rather than
/// the text pattern-matched.
fn is_permission_denied(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
    })
}

fn is_walk_permission_denied(error: &walkdir::Error) -> bool {
    // Only an actual refusal. `NotFound` is a path that vanished between the
    // readdir and the stat — a real error, but not one a TCC grant fixes, and
    // certainly not one to tell the user to grant Full Disk Access for.
    error
        .io_error()
        .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
}

/// The one line a root gets when part of it is unreadable: what happened, how
/// much of it, and what the user can actually do about it.
fn denied_summary(root: &Path, entries: usize) -> String {
    let entry = if entries == 1 { "entry" } else { "entries" };
    format!(
        "{}: {entries} {entry} could not be read (permission denied) — grant access \
         (macOS: System Settings → Privacy & Security → Full Disk Access) and Paper picks it \
         up on the next pass; retrying in {} minutes",
        root.display(),
        DENIED_BACKOFF.as_secs() / 60
    )
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
            start_line = line_no.saturating_sub(kept.len());
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

/// Whether OCR should run when the caller did not ask explicitly.
fn ocr_by_default() -> bool {
    #[cfg(feature = "ocr")]
    {
        crate::core::extract::ocr::enabled(false)
    }
    #[cfg(not(feature = "ocr"))]
    {
        false
    }
}
