use super::connection::Connection;
use anyhow::Result;

/// Embedding width of EmbeddingGemma. Kept as a constant so the schema and the
/// embedder can never silently disagree.
pub const EMBEDDING_DIMS: usize = 768;

/// Schema for `chunks.db`.
///
/// `chunks_fts` is an *external content* FTS5 table: the text lives only in
/// `chunks`, the FTS table just holds the index. The three triggers keep it in
/// sync. `chunks_vec` stores one 768-dim vector per chunk, keyed by `rowid`
/// which equals `chunks.id`.
pub fn schema_sql() -> String {
    format!(
        r#"
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
    embedding float[{EMBEDDING_DIMS}]
);

CREATE INDEX IF NOT EXISTS idx_chunks_path ON chunks(path);
CREATE INDEX IF NOT EXISTS idx_chunks_mtime ON chunks(mtime);
"#,
        EMBEDDING_DIMS = EMBEDDING_DIMS
    )
}

pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(&schema_sql())?;
    Ok(())
}
