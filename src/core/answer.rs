//! Extractive answering.
//!
//! Paper never invents text: an answer is always a verbatim quote from a local
//! document plus the citation that proves it. Generative polish is an optional,
//! separate step and is never required for a correct answer.

use crate::core::search::{Confidence, Hit};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Citation {
    pub source_id: i64,
    pub path: String,
    pub page: i64,
    pub line_start: i64,
    pub line_end: i64,
    /// Format-specific anchor, when the source has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// `path [anchor]:line_start-line_end`, or `path:line_start-line_end`.
    pub reference: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Answer {
    pub question: String,
    /// Verbatim extract supporting the answer.
    pub text: String,
    pub citations: Vec<Citation>,
    /// True when nothing relevant was found locally.
    pub not_found: bool,
    /// Set when a generative engine rewrote the extract. Empty for local answers.
    pub engine: String,
}

impl Citation {
    pub fn from_hit(hit: &Hit) -> Self {
        let mut reference = hit.path.clone();
        if let Some(anchor) = &hit.anchor {
            reference.push_str(&format!(" [{anchor}]"));
        }
        if hit.line_start > 0 {
            reference.push_str(&format!(":{}-{}", hit.line_start, hit.line_end));
        } else if hit.page > 0 {
            reference.push_str(&format!(":p.{}", hit.page));
        }
        Self {
            source_id: hit.source_id,
            path: hit.path.clone(),
            page: hit.page,
            line_start: hit.line_start,
            line_end: hit.line_end,
            anchor: hit.anchor.clone(),
            reference,
        }
    }
}

impl Answer {
    /// Build an answer from ranked hits.
    ///
    /// Hits whose evidence is [`Confidence::Low`] are treated as no answer at
    /// all: a plausible-looking quote from a weak match is worse than saying
    /// "not found", because it makes the user trust a wrong citation.
    pub fn from_hits(question: &str, hits: &[Hit]) -> Self {
        let supported: Vec<&Hit> = hits
            .iter()
            .filter(|hit| !matches!(hit.confidence, Confidence::Low))
            .collect();
        match supported.first() {
            None => Self {
                question: question.to_string(),
                text: String::new(),
                citations: vec![],
                not_found: true,
                engine: "local".to_string(),
            },
            Some(top) => Self {
                question: question.to_string(),
                text: top.snippet.clone(),
                citations: supported.iter().take(3).map(|h| Citation::from_hit(h)).collect(),
                not_found: false,
                engine: "local".to_string(),
            },
        }
    }

    /// Human-readable rendering used by the CLI.
    pub fn render(&self) -> String {
        if self.not_found {
            return "No matching document found in the local index.".to_string();
        }
        let mut out = self.text.clone();
        out.push_str("\n\nSources:\n");
        for citation in &self.citations {
            out.push_str(&format!("  - {}\n", citation.reference));
        }
        out
    }
}
