pub mod hit;
pub mod pipeline;
pub mod tier0;
pub mod tier1;

pub use hit::{Confidence, Engine, Hit};
pub use pipeline::{rrf_fuse, QueryEmbedder, SearchPipeline, SharedEmbedder};
pub use tier0::{tier0_search, HitSource, RawHit, MIN_QUERY_LEN};
pub use tier1::{tier1_search, serialize_f32_vec};
