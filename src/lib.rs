//! Paper — local-first document search with exact citations.
//!
//! One binary, two front-ends: a CLI and an MCP stdio server, both driven by
//! the same `core` library.

#[cfg(feature = "daemon")]
pub mod bootstrap;
pub mod cli;
pub mod core;
pub mod daemon;
pub mod mcp;
pub mod util;

pub use core::config::{Config, SearchConfig};
pub use core::index::{init_db, Connection, Indexer};
pub use core::models::{Embedder, ModelManager, Reranker};
pub use core::paths::Paths;
pub use core::search::{Hit, RawHit, SearchPipeline};
