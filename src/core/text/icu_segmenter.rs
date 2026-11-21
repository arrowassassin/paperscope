//! ICU Word Segmenter based segmentation for non-space scripts.
//!
//! Uses ICU4X's `WordSegmenter` to segment text at word boundaries for
//! scripts that don't use spaces between words: Chinese, Korean, Thai, Lao,
//! Khmer, Myanmar, Arabic, Hebrew, Devanagari, etc.

#[cfg(feature = "icu_segmenter")]
use icu_locid::Locale;
#[cfg(feature = "icu_segmenter")]
use icu_provider_adapters::fallback::{LocaleFallbackProvider, LocaleFallbacker};
#[cfg(feature = "icu_segmenter")]
use icu_provider::prelude::*;
#[cfg(feature = "icu_segmenter")]
use icu_segmenter::{WordSegmenter, ComplexPayloads};

/// Segment text using ICU's WordSegmenter for the given locale.
///
/// Returns a space-separated string of tokens suitable for FTS5 indexing.
/// Falls back to returning the original text if ICU segmentation fails.
#[cfg(feature = "icu_segmenter")]
pub fn icu_segment(text: &str, locale: &str) -> String {
    // Parse the locale
    let locale: Locale = locale.parse().unwrap_or_else(|_| "en".parse().unwrap());

    // Create a word segmenter using the locale fallback provider (bundled data)
    let fallbacker = LocaleFallbacker::new();
    let provider = LocaleFallbackProvider::try_new_unstable(fallbacker).ok();
    let payload = provider
        .and_then(|p| p.load(locale.into()).ok())
        .and_then(|p| p.take_payload().ok());

    let segmenter = match payload {
        Some(payload) => WordSegmenter {
            payload,
            complex: ComplexPayloads::new_auto(),
        },
        None => return text.to_string(), // Fallback to original text
    };

    // Iterate through word boundaries and collect tokens
    let mut tokens = Vec::new();
    let mut iter = segmenter.segment_str(text);
    let mut start = 0;

    for boundary in iter.by_ref() {
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