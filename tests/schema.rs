use paper::core::index::schema::SCHEMA_VERSION;
use paper::core::index::{init_db, Connection};

fn memory_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    init_db(&conn).unwrap();
    conn
}

fn insert_chunk(conn: &Connection, path: &str, content: &str) -> i64 {
    conn.inner()
        .execute(
            "INSERT INTO chunks (path, page, line_start, line_end, content, fts_text, content_hash, mtime, size, metadata)
             VALUES (?, 1, 1, 3, ?, ?, x'00', 1000, 100, '{}')",
            rusqlite::params![path, content, content],
        )
        .unwrap();
    conn.inner().last_insert_rowid()
}

#[test]
fn init_db_creates_all_tables() {
    let conn = memory_db();
    for table in ["chunks", "chunks_fts", "chunks_vec", "unreadable"] {
        let n = conn
            .inner()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = ?",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(n, 1, "missing table {table}");
    }
}

/// The `unreadable` table is additive: an existing v4 database gains it on open,
/// without a schema version bump or a reindex.
#[test]
fn an_existing_database_gains_the_unreadable_table_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chunks.db");
    {
        let conn = Connection::open(&path).unwrap();
        init_db(&conn).unwrap();
        conn.inner()
            .execute_batch("DROP TABLE unreadable;")
            .unwrap();
        let version: i64 = conn
            .inner()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION, "precondition: a current database");
    }
    let conn = Connection::open(&path).unwrap();
    init_db(&conn).unwrap();
    let n: i64 = conn
        .inner()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'unreadable'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "the table must be recreated without a version bump");
    let version: i64 = conn
        .inner()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION, "the version must not move");
}

#[test]
fn sqlite_vec_is_statically_available() {
    let conn = memory_db();
    let version: String = conn
        .inner()
        .query_row("SELECT vec_version()", [], |row| row.get(0))
        .unwrap();
    assert!(version.starts_with('v'), "unexpected version {version}");
}

#[test]
fn database_uses_wal_for_concurrent_readers() {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(&dir.path().join("t.db")).unwrap();
    init_db(&conn).unwrap();
    let mode: String = conn
        .inner()
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

#[test]
fn fts_triggers_index_and_remove_content() {
    let conn = memory_db();
    let id = insert_chunk(&conn, "/test/doc.txt", "warranty covers 24 months");
    let count = conn
        .query_i64("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'warranty'")
        .unwrap();
    assert_eq!(count, 1);

    conn.inner()
        .execute(
            "UPDATE chunks SET content = 'receipt total 42 dollars', fts_text = ? WHERE id = ?",
            rusqlite::params!["receipt total 42 dollars", id],
        )
        .unwrap();
    assert_eq!(
        conn.query_i64("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'warranty'")
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_i64("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'receipt'")
            .unwrap(),
        1
    );

    conn.inner()
        .execute("DELETE FROM chunks WHERE id = ?", [id])
        .unwrap();
    assert_eq!(
        conn.query_i64("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'receipt'")
            .unwrap(),
        0
    );
}

#[test]
fn init_db_is_idempotent() {
    let conn = memory_db();
    init_db(&conn).unwrap();
    init_db(&conn).unwrap();
    let id = insert_chunk(&conn, "/a.txt", "hello world");
    assert!(id > 0);
}

#[test]
fn vec_table_accepts_and_retrieves_embeddings() {
    let conn = memory_db();
    let id = insert_chunk(&conn, "/a.txt", "warranty covers 24 months");
    let vector = vec![0.5f32; paper::core::index::EMBEDDING_DIMS];
    conn.inner()
        .execute(
            "INSERT INTO chunks_vec (rowid, embedding) VALUES (?, ?)",
            rusqlite::params![id, paper::core::search::serialize_f32_vec(&vector)],
        )
        .unwrap();

    let (rowid, distance): (i64, f64) = conn
        .inner()
        .query_row(
            "SELECT rowid, distance FROM chunks_vec WHERE embedding MATCH ? AND k = 1",
            [paper::core::search::serialize_f32_vec(&vector)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(rowid, id);
    assert!(distance.abs() < 1e-5, "distance was {distance}");
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// The index holds a plaintext copy of every indexed document.
#[cfg(unix)]
#[test]
fn a_new_index_and_the_directory_created_for_it_are_owner_only() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("paper");
    let path = base.join("chunks.db");
    let conn = Connection::open(&path).unwrap();
    init_db(&conn).unwrap();
    assert_eq!(mode(&base), 0o700);
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(&base.join("chunks.db-wal")), 0o600);
}

#[cfg(unix)]
#[test]
fn an_existing_world_readable_index_is_made_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chunks.db");
    let first = Connection::open(&path).unwrap(); // stays open: -wal and -shm exist
    init_db(&first).unwrap();
    let files = ["chunks.db", "chunks.db-wal", "chunks.db-shm"].map(|f| dir.path().join(f));
    for file in &files {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let _second = Connection::open(&path).unwrap();
    for file in &files {
        assert_eq!(mode(file), 0o600, "{}", file.display());
    }
}
