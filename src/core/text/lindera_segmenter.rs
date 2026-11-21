//! Lindera (pure Rust) Japanese tokenizer with IPADIC dictionary.
//!
//! Uses `Mode::Decompose` (the successor of the old `TokenizeMode::Search`)
//! which is optimized for information retrieval: it produces more tokens,
//! improving search recall.
//!
//! The IPADIC dictionary is embedded in the binary (`embed-ipadic` feature),
//! so there is no build-time or run-time network access.

#[cfg(feature = "lindera")]
use lindera::dictionary::{load_embedded_dictionary, DictionaryKind};
#[cfg(feature = "lindera")]
use lindera::mode::{Mode, Penalty};
#[cfg(feature = "lindera")]
use lindera::segmenter::Segmenter;
#[cfg(feature = "lindera")]
use std::sync::OnceLock;

/// Shared segmenter: loading the embedded dictionary is expensive, so do it once.
#[cfg(feature = "lindera")]
fn segmenter() -> Option<&'static Segmenter> {
    static SEGMENTER: OnceLock<Option<Segmenter>> = OnceLock::new();
    SEGMENTER
        .get_or_init(|| {
            let dictionary = load_embedded_dictionary(DictionaryKind::IPADIC).ok()?;
            Some(Segmenter::new(
                Mode::Decompose(Penalty::default()),
                dictionary,
                None,
            ))
        })
        .as_ref()
}

/// Segment Japanese text using Lindera with IPADIC dictionary in decompose
/// (search) mode.
///
/// Returns a space-separated string of tokens suitable for FTS5 indexing.
/// Falls back to returning the original text if Lindera segmentation fails.
#[cfg(feature = "lindera")]
pub fn lindera_segment(text: &str) -> String {
    let Some(segmenter) = segmenter() else {
        return text.to_string();
    };

    let Ok(tokens) = segmenter.segment(std::borrow::Cow::Borrowed(text)) else {
        return text.to_string();
    };

    let tokens: Vec<&str> = tokens
        .iter()
        .map(|token| token.surface.as_ref())
        .filter(|t| !t.trim().is_empty())
        .collect();

    if tokens.is_empty() {
        return text.to_string();
    }

    tokens.join(" ")
}

/// Fallback when Lindera feature is not enabled
#[cfg(not(feature = "lindera"))]
pub fn lindera_segment(text: &str) -> String {
    text.to_string()
}
