use super::connection::Connection;
use anyhow::Result;

/// Embedding width of the supported models. Kept as a constant so the schema and
/// the embedder can never silently disagree.
pub use crate::core::models::specs::EMBEDDING_DIMS;

/// Current schema version. Bump this whenever `chunks` changes shape and add a
/// migration step in [`migrate`].
pub const SCHEMA_VERSION: i64 = 4;

/// Schema for `chunks.db`.
///
/// `chunks_fts` is an *external content* FTS5 table over the original text plus
/// a script-segmented fallback, so CJK/Thai text is searchable while displayed
/// citations keep the original bytes.
///
/// Neither column stores a second copy of `content` in the common case:
///
/// * `fts_text` holds the segmented form of `content` and is `''` when
///   segmentation is the identity (Latin, or any text without dense scripts).
/// * `fts_section` holds the segmented form of the section heading and is `''`
///   when segmentation is the identity.
///
/// The triggers therefore index `COALESCE(NULLIF(fts_text,''), content)` and
/// `COALESCE(NULLIF(fts_section,''), COALESCE(section,''))`. Every insert and
/// delete must use those exact expressions, so do not call FTS5's `rebuild`
/// command: it reads the raw columns and would drop the fallback.
///
/// `chunks_vec` stores one 384-dim vector per chunk keyed by `rowid`, which
/// equals `chunks.id`.
pub fn schema_sql() -> String {
    format!(
        r#"
CREATE TABLE IF NOT EXISTS chunks (
    id           INTEGER PRIMARY KEY,
    path         TEXT NOT NULL,
    source_path  TEXT NOT NULL DEFAULT '',
    page         INTEGER NOT NULL DEFAULT 0,
    line_start   INTEGER NOT NULL DEFAULT 0,
    line_end     INTEGER NOT NULL DEFAULT 0,
    anchor       TEXT,
    section      TEXT,
    content      TEXT NOT NULL,
    fts_text     TEXT NOT NULL DEFAULT '',
    fts_section  TEXT NOT NULL DEFAULT '',
    content_hash BLOB NOT NULL,
    mtime        INTEGER NOT NULL,
    size         INTEGER NOT NULL,
    metadata     TEXT
);

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    fts_text,
    fts_section,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, fts_text, fts_section)
        VALUES (new.id,
                COALESCE(NULLIF(new.fts_text, ''), new.content),
                COALESCE(NULLIF(new.fts_section, ''), COALESCE(new.section, '')));
END;
CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, fts_text, fts_section)
        VALUES('delete', old.id,
               COALESCE(NULLIF(old.fts_text, ''), old.content),
               COALESCE(NULLIF(old.fts_section, ''), COALESCE(old.section, '')));
END;
CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, fts_text, fts_section)
        VALUES('delete', old.id,
               COALESCE(NULLIF(old.fts_text, ''), old.content),
               COALESCE(NULLIF(old.fts_section, ''), COALESCE(old.section, '')));
    INSERT INTO chunks_fts(rowid, fts_text, fts_section)
        VALUES (new.id,
                COALESCE(NULLIF(new.fts_text, ''), new.content),
                COALESCE(NULLIF(new.fts_section, ''), COALESCE(new.section, '')));
END;

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_vec USING vec0(
    embedding float[{EMBEDDING_DIMS}]
);

CREATE TABLE IF NOT EXISTS index_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chunks_path ON chunks(path);
CREATE INDEX IF NOT EXISTS idx_chunks_source ON chunks(source_path);
CREATE INDEX IF NOT EXISTS idx_chunks_mtime ON chunks(mtime);
CREATE INDEX IF NOT EXISTS idx_chunks_source_path_mtime ON chunks(source_path, mtime);
CREATE INDEX IF NOT EXISTS idx_chunks_content_hash ON chunks(content_hash);
CREATE INDEX IF NOT EXISTS idx_chunks_page ON chunks(page);
"#,
        EMBEDDING_DIMS = EMBEDDING_DIMS
    )
}

pub fn init_db(conn: &Connection) -> Result<()> {
    let preexisting = table_exists(conn, "chunks")?;
    conn.execute_batch(&schema_sql())?;
    if !preexisting {
        // Fresh database: run integrity_check (expensive but thorough) before
        // marking the schema version. On a brand-new file this is instant.
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            anyhow::bail!("database integrity_check failed: {integrity}");
        }
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        return Ok(());
    }
    migrate(conn)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [&name as &dyn rusqlite::ToSql],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Upgrade a database created by an older 0.x build. Fresh databases already
/// carry `user_version = SCHEMA_VERSION` and skip this entirely.
fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }

    // Migration from v1 to v2 (original 0.x -> 1.x upgrade)
    if version < 2 {
        add_column_if_missing(conn, "source_path", "TEXT NOT NULL DEFAULT ''")?;
        add_column_if_missing(conn, "anchor", "TEXT")?;
        add_column_if_missing(conn, "section", "TEXT")?;
        add_column_if_missing(conn, "fts_text", "TEXT NOT NULL DEFAULT ''")?;

        // The old FTS table indexed `content`; rebuild it over `fts_text`.
        conn.execute_batch(
            r#"
BEGIN;
UPDATE chunks SET source_path = path WHERE source_path = '';
UPDATE chunks SET fts_text = content WHERE fts_text = '';
DROP TRIGGER IF EXISTS chunks_ai;
DROP TRIGGER IF EXISTS chunks_ad;
DROP TRIGGER IF EXISTS chunks_au;
DROP TABLE IF EXISTS chunks_fts;
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    fts_text,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);
INSERT INTO chunks_fts(rowid, fts_text) SELECT id, fts_text FROM chunks;
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, fts_text) VALUES (new.id, new.fts_text);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, fts_text) VALUES('delete', old.id, old.fts_text);
END;
CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, fts_text) VALUES('delete', old.id, old.fts_text);
    INSERT INTO chunks_fts(rowid, fts_text) VALUES (new.id, new.fts_text);
END;
CREATE INDEX IF NOT EXISTS idx_chunks_source ON chunks(source_path);
PRAGMA user_version = 2;
COMMIT;
"#,
        )?;
    }

    // Migration from v2 to v3: add missing indexes for common query patterns
    if version < 3 {
        conn.execute_batch(
            r#"
BEGIN;
-- Speed up DISTINCT source_path scans used by prune_missing and dedup
CREATE INDEX IF NOT EXISTS idx_chunks_source_path_mtime ON chunks(source_path, mtime);
-- Speed up hash-based dedup lookups
CREATE INDEX IF NOT EXISTS idx_chunks_content_hash ON chunks(content_hash);
-- Speed up page-scoped queries
CREATE INDEX IF NOT EXISTS idx_chunks_page ON chunks(page);
PRAGMA user_version = 3;
COMMIT;
"#,
        )?;
    }

    // Migration from v3 to v4: stop storing a second copy of `content` in
    // `fts_text`. Before this, every chunk stored `fts_text = content` (plus the
    // section heading) even though segmentation is the identity for Latin text.
    // Now `fts_text`/`fts_section` only hold genuinely segmented text and the
    // triggers fall back to `content`/`section`.
    if version < 4 {
        add_column_if_missing(conn, "fts_section", "TEXT NOT NULL DEFAULT ''")?;
        conn.execute_batch(
            r#"
BEGIN;
DROP TRIGGER IF EXISTS chunks_ai;
DROP TRIGGER IF EXISTS chunks_ad;
DROP TRIGGER IF EXISTS chunks_au;
DROP TABLE IF EXISTS chunks_fts;
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    fts_text,
    fts_section,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);
UPDATE chunks
   SET fts_text = ''
 WHERE fts_text = content
    OR fts_text = COALESCE(section, '') || char(10) || content;
INSERT INTO chunks_fts(rowid, fts_text, fts_section)
    SELECT id,
           COALESCE(NULLIF(fts_text, ''), content),
           COALESCE(NULLIF(fts_section, ''), COALESCE(section, ''))
      FROM chunks;
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, fts_text, fts_section)
        VALUES (new.id,
                COALESCE(NULLIF(new.fts_text, ''), new.content),
                COALESCE(NULLIF(new.fts_section, ''), COALESCE(new.section, '')));
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, fts_text, fts_section)
        VALUES('delete', old.id,
               COALESCE(NULLIF(old.fts_text, ''), old.content),
               COALESCE(NULLIF(old.fts_section, ''), COALESCE(old.section, '')));
END;
CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, fts_text, fts_section)
        VALUES('delete', old.id,
               COALESCE(NULLIF(old.fts_text, ''), old.content),
               COALESCE(NULLIF(old.fts_section, ''), COALESCE(old.section, '')));
    INSERT INTO chunks_fts(rowid, fts_text, fts_section)
        VALUES (new.id,
                COALESCE(NULLIF(new.fts_text, ''), new.content),
                COALESCE(NULLIF(new.fts_section, ''), COALESCE(new.section, '')));
END;
PRAGMA user_version = 4;
COMMIT;
"#,
        )?;
    }

    Ok(())
}

fn add_column_if_missing(conn: &Connection, name: &str, decl: &str) -> Result<()> {
    let existing: Vec<String> = {
        let mut stmt = conn.inner().prepare("PRAGMA table_info(chunks)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out
    };
    if !existing.iter().any(|c| c == name) {
        conn.execute_batch(&format!("ALTER TABLE chunks ADD COLUMN {name} {decl};"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A v3 database: same tables, but the FTS triggers still index the raw
    /// `fts_text` column and Latin rows duplicate `content` there.
    fn write_v3_database(path: &std::path::Path) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            r#"
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY, path TEXT NOT NULL, source_path TEXT NOT NULL DEFAULT '',
    page INTEGER NOT NULL DEFAULT 0, line_start INTEGER NOT NULL DEFAULT 0,
    line_end INTEGER NOT NULL DEFAULT 0, anchor TEXT, section TEXT,
    content TEXT NOT NULL, fts_text TEXT NOT NULL DEFAULT '',
    content_hash BLOB NOT NULL, mtime INTEGER NOT NULL, size INTEGER NOT NULL,
    metadata TEXT
);
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    fts_text, content='chunks', content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, fts_text) VALUES (new.id, new.fts_text);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, fts_text) VALUES('delete', old.id, old.fts_text);
END;
CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, fts_text) VALUES('delete', old.id, old.fts_text);
    INSERT INTO chunks_fts(rowid, fts_text) VALUES (new.id, new.fts_text);
END;
INSERT INTO chunks
    (id, path, source_path, page, line_start, line_end, content, fts_text,
     content_hash, mtime, size)
VALUES
    (1, 'a.txt', 'a.txt', 0, 1, 1, 'fridge warranty text',
     'fridge warranty text', x'00', 0, 0);
PRAGMA user_version = 3;
"#,
        )
        .unwrap();
    }

    #[test]
    fn v3_to_v4_migration_keeps_existing_rows_searchable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chunks.db");
        write_v3_database(&path);

        let conn = Connection::open(&path).unwrap();
        init_db(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);

        let fts_text: String = conn
            .query_row("SELECT fts_text FROM chunks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fts_text, "", "duplicate copy must be dropped");

        let matches: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'warranty'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            matches, 1,
            "the fallback to content must keep the row indexed"
        );

        // Deleting through the new trigger must not corrupt the index.
        conn.execute_batch("DELETE FROM chunks WHERE id = 1;")
            .unwrap();
        let after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'warranty'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after, 0);
    }
}
