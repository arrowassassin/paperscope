pub mod connection;
pub mod indexer;
pub mod schema;

pub use connection::{register_sqlite_vec, Connection};
pub use indexer::{chunk_blocks, chunk_text, reembed_pending, Chunk, IndexStats, Indexer};
pub use schema::{init_db, EMBEDDING_DIMS};
