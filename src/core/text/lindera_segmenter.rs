//! Lindera (pure Rust) Japanese tokenizer with IPADIC dictionary.
//!
//! Uses `TokenizeMode::Search` which is optimized for information retrieval
//! (produces more tokens, better for search recall).

#[cfg(feature = "lindera")]
use lindera::tokenizer::{Tokenizer, TokenizeMode};
#[cfg(feature = "lindera")]
use lindera::dictionary::load_dictionary;
#[cfg(feature = "lindera")]
use lindera_ipadic::LinderaIpadicBuilder;

/// Segment Japanese text using Lindera with IPADIC dictionary in Search mode.
///
/// Returns a space-separated string of tokens suitable for FTS5 indexing.
/// Falls back to returning the original text if Lindera segmentation fails.
#[cfg(feature = "lindera")]
pub fn lindera_segment(text: &str) -> String {
    // Load the IPADIC dictionary
    let dictionary = match load_dictionary(LinderaIpadicBuilder::default()) {
        Ok(dict) => dict,
        Err(_) => return text.to_string(), // Fallback to original text
    };

    // Build the tokenizer with IPADIC dictionary
    let tokenizer = match Tokenizer::new(dictionary, TokenizeMode::Search) {
        Ok(t) => t,
        Err(_) => return text.to_string(), // Fallback to original text
    };

    // Tokenize the text
    let tokens: Vec<String> = tokenizer
        .tokenize(text)
        .into_iter()
        .map(|token| token.text.to_string())
        .filter(|t| !t.trim().is_empty())
        .collect();

    // If no tokens found, return original text
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