use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub source_id: i64,
    pub path: String,
    /// The file on disk this chunk came from. Differs from `path` for archive
    /// members, which are addressed as `outer.zip!/inner/path`.
    #[serde(default)]
    pub source_path: String,
    /// 1-based page for paged formats, 0 when the format has no pages.
    pub page: i64,
    pub line_start: i64,
    pub line_end: i64,
    /// Format-specific citation anchor: `page 3`, `Sheet1!A5:H5`, `slide 2`,
    /// `para 4-9`, `spine 1: chapter.xhtml`, `message <id>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    pub snippet: String,
    /// Full chunk text. Kept separate from `snippet` (which is truncated for
    /// display) because confidence checks and the cross-encoder need the whole
    /// passage, not the first 240 characters.
    pub content: String,
    pub score: f32,
    pub why_matched: String,
    pub confidence: Confidence,
    pub engine: Engine,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Engine {
    Local,
}
