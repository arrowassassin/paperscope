pub mod filters;
pub mod hit;
pub mod pipeline;
pub mod tier0;
pub mod tier1;

pub use filters::SearchFilters;
pub use hit::{Confidence, Engine, Hit};
pub use pipeline::{rrf_fuse, QueryEmbedder, SearchPipeline, SharedEmbedder};
pub use tier0::{tier0_search, tier0_search_filtered, HitSource, RawHit, MIN_QUERY_LEN};
pub use tier1::{serialize_f32_vec, tier1_search, tier1_search_filtered};
