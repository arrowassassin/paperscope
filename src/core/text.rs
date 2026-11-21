//! Script-aware tokenisation and deterministic excerpting.
//!
//! SQLite's `unicode61` tokenizer treats a run of CJK, Thai, Lao, Khmer or
//! Myanmar characters as a *single* token, so `保証` never matches inside
//! `保証期間は24ヶ月です`. The fix is applied identically when indexing and when
//! querying:
//!
//! - **CJK runs** become overlapping character bigrams plus the first character
//!   of the run (`全文檢索` → `全 全文 文檢 檢索`). This is the best-performing
//!   dictionary-free scheme measured across Chinese, Japanese and Korean; it
//!   keeps two-character words searchable, which `trigram` cannot, and it needs
//!   no dictionary, so it is not tied to one language variant.
//! - **Thai, Lao, Khmer, Myanmar** have no spaces either but are not made of
//!   bigram-sized units, so each character is emitted as a token.
//! - **Everything else** is left to `unicode61`.
//!
//! Optional dictionary segmenters:
//! - Jieba (feature `jieba-rs`) for Chinese word-level segmentation
//! - ICU (feature `icu_segmenter`) for word boundaries in CJK, Korean, Thai,
//!   Lao, Khmer, Myanmar, Arabic, Hebrew, Devanagari, etc.
//! - Lindera (feature `lindera`) for Japanese with IPADIC dictionary
//!
//! All optional segmenters are opt-in and default off to keep the binary small
//! and dependency-free.
//!
//! No stemming: it would be a guess, and a guess changes the bytes a citation
//! points at.

use unicode_normalization::UnicodeNormalization;

#[cfg(feature = "icu_segmenter")]
mod icu_segmenter;
#[cfg(feature = "icu_segmenter")]
use icu_segmenter::icu_segment;

#[cfg(feature = "lindera")]
mod lindera_segmenter;
#[cfg(feature = "lindera")]
use lindera_segmenter::lindera_segment;

/// A segmenter splits text into tokens for FTS5 indexing and querying.
///
/// Implementations should be deterministic and produce the same tokens for
/// identical input. The tokens are joined with spaces to form the FTS5 input.
pub trait Segmenter: Send + Sync {
    /// Segment text into tokens. Returns an iterator over owned token strings.
    fn segment(&self, text: &str) -> Box<dyn Iterator<Item = String> + '_>;

    /// Name of the segmenter (for diagnostics).
    fn name(&self) -> &'static str;
}

// Auto-implement Segmenter for Box<dyn Segmenter> so we can store boxed segmenters
impl Segmenter for Box<dyn Segmenter> {
    fn segment(&self, text: &str) -> Box<dyn Iterator<Item = String> + '_> {
        self.as_ref().segment(text)
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

/// The default bigram+first-char segmenter (no dictionary, works for all CJK).
pub struct BigramFirstCharSegmenter;

impl Segmenter for BigramFirstCharSegmenter {
    fn segment(&self, text: &str) -> Box<dyn Iterator<Item = String> + '_> {
        let tokens = collect_bigram_first_char(text);
        Box::new(tokens.into_iter())
    }

    fn name(&self) -> &'static str {
        "bigram-first-char"
    }
}

/// Jieba Chinese segmenter (feature `jieba-rs`).
#[cfg(feature = "jieba-rs")]
pub struct JiebaSegmenter {
    jieba: jieba_rs::Jieba,
}

#[cfg(feature = "jieba-rs")]
impl Default for JiebaSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "jieba-rs")]
impl JiebaSegmenter {
    pub fn new() -> Self {
        Self {
            jieba: jieba_rs::Jieba::new(),
        }
    }
}

#[cfg(feature = "jieba-rs")]
impl Segmenter for JiebaSegmenter {
    fn segment(&self, text: &str) -> Box<dyn Iterator<Item = String> + '_> {
        let tokens: Vec<String> = self
            .jieba
            .cut(text, false)
            .into_iter()
            .map(String::from)
            .collect();
        Box::new(tokens.into_iter())
    }

    fn name(&self) -> &'static str {
        "jieba"
    }
}

/// Global segmenter instance (initialized once).
static SEGMENTER: std::sync::OnceLock<Box<dyn Segmenter>> = std::sync::OnceLock::new();

/// Initialize the global segmenter. Call once at startup.
/// If no feature is enabled, falls back to BigramFirstCharSegmenter.
pub fn init_segmenter() -> &'static dyn Segmenter {
    SEGMENTER.get_or_init(|| {
        #[cfg(feature = "jieba-rs")]
        {
            return Box::new(JiebaSegmenter::new()) as Box<dyn Segmenter>;
        }
        #[allow(unreachable_code)]
        Box::new(BigramFirstCharSegmenter)
    })
}

/// Get the current segmenter (initializes if needed).
pub fn segmenter() -> &'static dyn Segmenter {
    init_segmenter()
}

/// Scripts written without spaces between words, indexed as characters or
/// bigrams. Returns 1 for bigram-unit scripts (Han, kana, Hangul), 2 for
/// character-unit scripts (Thai, Lao, Khmer, Myanmar).
fn dense_script(c: char) -> Option<u8> {
    match c as u32 {
        0x0E00..=0x0E7F | // Thai
        0x0E80..=0x0EFF | // Lao
        0x1000..=0x109F | // Myanmar
        0x1780..=0x17FF   // Khmer
            => Some(2),
        0x2E80..=0x2FFF |   // CJK radicals, Kangxi
        0x3000..=0x303F |   // CJK symbols and punctuation
        0x3040..=0x30FF |   // Hiragana + Katakana
        0x3400..=0x4DBF |   // CJK Extension A
        0x4E00..=0x9FFF |   // CJK Unified Ideographs
        0xAC00..=0xD7AF |   // Hangul syllables
        0xF900..=0xFAFF |   // CJK Compatibility Ideographs
        0x20000..=0x2FA1F   // CJK Extensions B–F, compatibility supplement
            => Some(1),
        _ => None,
    }
}

/// True when `c` belongs to a script indexed without spaces.
pub fn is_dense_script(c: char) -> bool {
    dense_script(c).is_some()
}

/// True when a query is long enough to be worth running.
///
/// Latin-script queries need three characters, but `検索` is a complete and
/// common word, so any query containing a dense-script character is allowed at
/// one character. This is a property of the script, not a language guess.
pub fn is_searchable_query(query: &str) -> bool {
    let mut chars = query.chars().filter(|c| !c.is_whitespace());
    match chars.next() {
        None => false,
        Some(c) if is_dense_script(c) => true,
        Some(_) => query.chars().filter(|c| !c.is_whitespace()).count() >= 3,
    }
}

/// Collect bigram+first-char tokens for the default segmenter.
///
/// This replicates the original `fts_segment` behavior: only adds token
/// boundaries for dense scripts (CJK, Thai, etc.), leaves Latin/other text
/// as whole words.
fn collect_bigram_first_char(text: &str) -> Vec<String> {
    let normalised: String = text.nfkc().collect();
    let mut tokens = Vec::new();
    let mut run: Vec<char> = Vec::new();
    let mut word: String = String::new(); // Accumulate non-dense-script words

    fn flush_run(run: &mut Vec<char>, tokens: &mut Vec<String>) {
        if run.is_empty() {
            return;
        }
        if let Some(kind) = run.first().and_then(|c| dense_script(*c)) {
            match kind {
                2 => {
                    // Character-unit scripts (Thai, Lao, Khmer, Myanmar): one token per char
                    for c in run.iter() {
                        tokens.push(c.to_string());
                    }
                }
                _ => {
                    // Bigram-unit scripts (CJK): first char, then overlapping bigrams
                    if let Some(first) = run.first() {
                        tokens.push(first.to_string());
                    }
                    for pair in run.windows(2) {
                        let mut s = String::new();
                        s.push(pair[0]);
                        s.push(pair[1]);
                        tokens.push(s);
                    }
                }
            }
        }
        run.clear();
    }

    for c in normalised.chars() {
        if dense_script(c).is_some() {
            // Dense script character: flush any pending word first
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            // Accumulate dense script characters into run
            run.push(c);
        } else {
            // Non-dense character: flush any dense-script run
            flush_run(&mut run, &mut tokens);
            if c.is_whitespace() {
                // Whitespace separates words but is not emitted as a token
                if !word.is_empty() {
                    tokens.push(std::mem::take(&mut word));
                }
            } else {
                // Part of a word
                word.push(c);
            }
        }
    }
    if !word.is_empty() {
        tokens.push(std::mem::take(&mut word));
    }
    flush_run(&mut run, &mut tokens);
    tokens
}

/// Detect the primary language/script of the text for ICU segmentation.
///
/// Returns an ICU locale code (e.g., "zh", "ja", "ko", "th", "lo", "km", "my",
/// "ar", "he", "hi") based on the dominant script in the text.
/// Returns None if no supported dense script is detected.
pub fn detect_language(text: &str) -> Option<&'static str> {
    // Count characters per script to find the dominant one
    let mut script_counts: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();

    for c in text.chars() {
        let script = match c as u32 {
            // Chinese (Han)
            0x2E80..=0x2FFF |   // CJK radicals, Kangxi
            0x3000..=0x303F |   // CJK symbols and punctuation
            0x3400..=0x4DBF |   // CJK Extension A
            0x4E00..=0x9FFF |   // CJK Unified Ideographs
            0xF900..=0xFAFF |   // CJK Compatibility Ideographs
            0x20000..=0x2FA1F   // CJK Extensions B–F, compatibility supplement
                => "zh",
            // Japanese (Hiragana + Katakana)
            0x3040..=0x30FF
                => "ja",
            // Korean (Hangul)
            0xAC00..=0xD7AF
                => "ko",
            // Thai
            0x0E00..=0x0E7F
                => "th",
            // Lao
            0x0E80..=0x0EFF
                => "lo",
            // Myanmar
            0x1000..=0x109F
                => "my",
            // Khmer
            0x1780..=0x17FF
                => "km",
            // Arabic
            0x0600..=0x06FF |   // Arabic
            0x0750..=0x077F |   // Arabic Supplement
            0x08A0..=0x08FF |   // Arabic Extended-A
            0xFB50..=0xFDFF |   // Arabic Presentation Forms-A
            0xFE70..=0xFEFF     // Arabic Presentation Forms-B
                => "ar",
            // Hebrew
            0x0590..=0x05FF
                => "he",
            // Devanagari
            0x0900..=0x097F
                => "hi",
            _ => continue,
        };
        *script_counts.entry(script).or_insert(0) += 1;
    }

    // Return the script with the highest count
    script_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(script, _)| script)
}

/// Check if text contains Japanese characters (Hiragana or Katakana).
#[cfg(feature = "lindera")]
fn is_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        let cp = c as u32;
        (0x3040..=0x309F).contains(&cp) || // Hiragana
        (0x30A0..=0x30FF).contains(&cp) // Katakana
    })
}

/// Normalise text and add token boundaries for scripts `unicode61` cannot split.
/// Uses the configured segmenter (default: bigram+first-char).
/// For the default segmenter, uses the original logic that only adds boundaries
/// for dense scripts (CJK, Thai, etc.) and leaves Latin/other text unchanged.
pub fn fts_segment(text: &str) -> String {
    // Check if we're using a non-default segmenter (ICU, Lindera, Jieba)
    // If so, use the segmenter; otherwise use the original logic
    let seg = segmenter();
    match seg.name() {
        "bigram-first-char" => {
            // Try ICU segmenter first (covers most scripts)
            #[cfg(feature = "icu_segmenter")]
            if let Some(lang) = detect_language(text) {
                return icu_segment(text, lang);
            }
            // Try Lindera for Japanese
            #[cfg(feature = "lindera")]
            if is_japanese(text) {
                return lindera_segment(text);
            }
            // Fallback to original bigram logic
            let tokens = collect_bigram_first_char(text);
            tokens.join(" ")
        }
        _ => {
            // Use the configured segmenter (Jieba, etc.)
            let tokens = seg.segment(text);
            tokens.collect::<Vec<_>>().join(" ")
        }
    }
}

/// True when two strings carry the same whitespace-separated tokens.
///
/// Segmentation normalises whitespace and FTS5 tokenises newlines and spaces
/// identically, so `a\nb` and `a b` produce the same index. Comparing tokens
/// rather than bytes is what lets the indexer skip storing a second copy of
/// text that segmentation did not actually change.
pub fn same_tokens(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

/// Split a query into FTS terms after segmentation (same boundaries as the index).
pub fn query_terms(query: &str) -> Vec<String> {
    fts_segment(query)
        .split_whitespace()
        .map(|t| t.to_string())
        .collect()
}

/// Characters shown in a search-result snippet.
pub const SNIPPET_CHARS: usize = 240;

/// A verbatim window of `text` around the first query term it contains.
pub fn excerpt(text: &str, terms: &[String], max_chars: usize) -> String {
    let collapsed = collapse_whitespace(text);
    let chars: Vec<char> = collapsed.chars().collect();
    if chars.len() <= max_chars {
        return collapsed;
    }

    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let mut hit: Option<usize> = None;
    for term in terms {
        let needle: Vec<char> = term
            .chars()
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .collect();
        if needle.is_empty() {
            continue;
        }
        if let Some(pos) = lower
            .windows(needle.len())
            .position(|window| window == needle.as_slice())
        {
            hit = Some(hit.map_or(pos, |best: usize| best.min(pos)));
        }
    }

    let start = hit
        .map(|pos| pos.saturating_sub(max_chars / 3))
        .unwrap_or(0);
    let end = (start + max_chars).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bigram fallback only applies when the ICU feature is off.
    #[cfg(not(feature = "icu_segmenter"))]
    #[test]
    fn cjk_runs_become_bigrams() {
        let s = fts_segment("全文檢索");
        let tokens: Vec<&str> = s.split_whitespace().collect();
        assert_eq!(tokens, ["全", "全文", "文檢", "檢索"]);
    }

    /// With ICU, CJK text is segmented into words instead of bigrams.
    #[cfg(feature = "icu_segmenter")]
    #[test]
    fn cjk_runs_become_words() {
        let s = fts_segment("全文檢索");
        let tokens: Vec<&str> = s.split_whitespace().collect();
        assert!(tokens.contains(&"檢索"), "{tokens:?}");
        assert!(
            !tokens.contains(&"文檢"),
            "no cross-word bigrams under ICU: {tokens:?}"
        );
    }

    #[test]
    fn two_character_words_survive() {
        let doc = fts_segment("南投縣魚池鄉");
        let query = fts_segment("魚池");
        let query_tokens: Vec<&str> = query.split_whitespace().collect();
        let doc_tokens: Vec<&str> = doc.split_whitespace().collect();
        assert!(
            query_tokens.iter().any(|qt| doc_tokens.contains(qt)),
            "query tokens: {:?}, doc tokens: {:?}",
            query_tokens,
            doc_tokens
        );
    }

    #[test]
    fn leaves_latin_alone() {
        assert_eq!(fts_segment("fridge warranty 2027"), "fridge warranty 2027");
    }

    #[test]
    fn normalises_full_width() {
        assert_eq!(fts_segment("ＷＡＲＲＡＮＴＹ"), "WARRANTY");
    }

    /// Per-character fallback only applies when the ICU feature is off.
    #[cfg(not(feature = "icu_segmenter"))]
    #[test]
    fn thai_is_indexed_per_character() {
        let s = fts_segment("การรับประกัน");
        assert!(s.split_whitespace().count() > 3, "{s}");
    }

    /// With ICU, Thai is segmented at dictionary word boundaries.
    #[cfg(feature = "icu_segmenter")]
    #[test]
    fn thai_is_indexed_per_word() {
        let s = fts_segment("การรับประกัน");
        let tokens: Vec<&str> = s.split_whitespace().collect();
        assert!(tokens.contains(&"การ"), "{tokens:?}");
        assert!(tokens.contains(&"รับประกัน"), "{tokens:?}");
    }

    #[test]
    fn short_dense_queries_are_searchable() {
        assert!(is_searchable_query("検索"));
        assert!(is_searchable_query("法"));
        assert!(!is_searchable_query("ab"));
        assert!(!is_searchable_query("   "));
        assert!(is_searchable_query("tax"));
    }

    #[test]
    fn excerpt_is_a_verbatim_window() {
        let text = "alpha beta gamma delta epsilon zeta eta theta";
        let out = excerpt(text, &["delta".to_string()], 24);
        assert!(out.contains("delta"), "{out}");
        assert!(text.contains(out.trim_matches('…')), "{out}");
    }

    #[test]
    fn excerpt_leaves_short_text_whole() {
        assert_eq!(
            excerpt("fridge warranty", &["warranty".to_string()], 240),
            "fridge warranty"
        );
    }

    #[test]
    fn segmenter_name_default() {
        assert_eq!(segmenter().name(), "bigram-first-char");
    }
}
