use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Once;

static REGISTER_VEC: Once = Once::new();

/// Register `sqlite-vec` as a SQLite auto-extension exactly once per process.
///
/// The C extension is compiled statically into the binary (no runtime `.dylib`
/// download, no `load_extension` call), which keeps `paper` a single binary.
#[allow(clippy::missing_transmute_annotations)]
pub fn register_sqlite_vec() {
    REGISTER_VEC.call_once(|| unsafe {
        // SAFETY: `sqlite3_vec_init` has the exact C signature SQLite expects for
        // an extension entry point; the transmute is the pattern shipped in the
        // upstream sqlite-vec crate's own tests.
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

/// Thin wrapper around `rusqlite::Connection` that guarantees the sqlite-vec
/// extension is available and applies Paper's pragmas.
pub struct Connection {
    conn: rusqlite::Connection,
}

impl Connection {
    /// Open an existing database file, running `PRAGMA quick_check` to verify
    /// integrity before any work proceeds. If the file doesn't exist, it is
    /// created with creation-time pragmas (page_size, etc.).
    pub fn open(path: &Path) -> Result<Self> {
        register_sqlite_vec();
        let existed = path.exists();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating db directory {}", parent.display()))?;
        }
        let conn = rusqlite::Connection::open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        // quick_check is fast and catches most corruption; fail fast if not ok.
        let quick_check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if quick_check != "ok" {
            anyhow::bail!("database quick_check failed: {quick_check}");
        }
        Self::finish(conn, !existed) // set page_size only on fresh files
    }

    /// Open a database file that will be created if it doesn't exist. Runs
    /// `PRAGMA quick_check` on the new/empty file (always "ok") and applies
    /// creation-time pragmas like `page_size`.
    /// 
    /// This is a convenience alias for `open`; both now handle fresh files.
    pub fn open_or_create(path: &Path) -> Result<Self> {
        Self::open(path)
    }

    pub fn open_in_memory() -> Result<Self> {
        register_sqlite_vec();
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::finish(conn, false) // in-memory DB cannot set page_size
    }

    /// Apply Paper's standard pragmas. `allow_page_size` controls whether
    /// `PRAGMA page_size = 8192` is executed (only on fresh files, never on
    /// existing databases — changing it requires VACUUM).
    fn finish(conn: rusqlite::Connection, allow_page_size: bool) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // WAL lets searches run while the indexer writes.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA mmap_size = 268435456;
             PRAGMA wal_autocheckpoint = 1000;",
        )?;
        if allow_page_size {
            // page_size must be set before any tables are created; it only takes
            // effect on a fresh file. On an existing DB this is a no-op.
            conn.execute_batch("PRAGMA page_size = 8192;")?;
        }
        Ok(Self { conn })
    }

    pub fn inner(&self) -> &rusqlite::Connection {
        &self.conn
    }

    pub fn inner_mut(&mut self) -> &mut rusqlite::Connection {
        &mut self.conn
    }

    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        self.conn.execute_batch(sql)?;
        Ok(())
    }

    /// Convenience for scalar queries used by doctor/tests.
    pub fn query_i64(&self, sql: &str) -> Result<i64> {
        Ok(self.conn.query_row(sql, [], |row| row.get(0))?)
    }

    pub fn query_row<T, P: rusqlite::Params>(&self, sql: &str, params: P, f: impl FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>) -> Result<T> {
        Ok(self.conn.query_row(sql, params, f)?)
    }
}
