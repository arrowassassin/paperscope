//! ICU Word Segmenter based segmentation for non-space scripts.
//!
//! Uses ICU4X's `WordSegmenter` to segment text at word boundaries for
//! scripts that don't use spaces between words: Chinese, Korean, Thai, Lao,
//! Khmer, Myanmar, Arabic, Hebrew, Devanagari, etc.

#[cfg(feature = "icu_segmenter")]
use icu_segmenter::WordSegmenter;

/// Segment text using ICU's WordSegmenter for the given locale.
///
/// Returns a space-separated string of tokens suitable for FTS5 indexing.
/// Falls back to returning the original text if ICU segmentation fails.
#[cfg(feature = "icu_segmenter")]
pub fn icu_segment(text: &str, _locale: &str) -> String {
    // With the `compiled_data` feature the constructor is infallible.
    let segmenter = WordSegmenter::new_auto();

    // Iterate through word boundaries and collect tokens
    let mut tokens = Vec::new();
    let mut start = 0;

    for boundary in segmenter.segment_str(text) {
        if boundary > start {
            let token = &text[start..boundary];
            // Filter out whitespace-only tokens
            if !token.trim().is_empty() {
                tokens.push(token.to_string());
            }
        }
        start = boundary;
    }

    // Handle remaining text after last boundary
    if start < text.len() {
        let token = &text[start..];
        if !token.trim().is_empty() {
            tokens.push(token.to_string());
        }
    }

    // If no tokens found, return original text
    if tokens.is_empty() {
        return text.to_string();
    }

    tokens.join(" ")
}

/// Fallback when ICU feature is not enabled
#[cfg(not(feature = "icu_segmenter"))]
pub fn icu_segment(text: &str, _locale: &str) -> String {
    text.to_string()
}
