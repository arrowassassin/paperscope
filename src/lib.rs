//! Paper — local-first document search with exact citations.
//!
//! One binary, two front-ends: a CLI and an MCP stdio server, both driven by
//! the same `core` library.

pub mod cli;
pub mod core;
pub mod daemon;
pub mod mcp;
pub mod util;

pub use core::config::{Config, SearchConfig};
pub use core::eval::{
    load_golden_set, make_eval_pipeline, run_eval, EvalError, EvalReport, GoldenDocument,
    GoldenQuery, GoldenSet,
};
pub use core::index::{init_db, Connection, Indexer};
pub use core::models::{Embedder, ModelManager, Reranker};
pub use core::paths::Paths;
pub use core::search::{Hit, RawHit, SearchPipeline};
