use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub source_id: i64,
    pub path: String,
    pub page: i64,
    pub line_start: i64,
    pub line_end: i64,
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
pub enum Confidence { High, Medium, Low }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Engine { Local, AppleFM, WindowsAion, GeminiNano, OpenRouter }