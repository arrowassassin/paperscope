use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Once;

static REGISTER_VEC: Once = Once::new();

/// Register `sqlite-vec` as a SQLite auto-extension exactly once per process.
///
/// The C extension is compiled statically into the binary (no runtime `.dylib`
/// download, no `load_extension` call), which keeps `paper` a single binary.
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
    pub fn open(path: &Path) -> Result<Self> {
        register_sqlite_vec();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating db directory {}", parent.display()))?;
        }
        let conn = rusqlite::Connection::open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        Self::finish(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        register_sqlite_vec();
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::finish(conn)
    }

    fn finish(conn: rusqlite::Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // WAL lets searches run while the indexer writes.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA mmap_size = 268435456;",
        )?;
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
}
