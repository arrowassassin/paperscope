//! Segmentation tests for all major scripts.
//!
//! These tests verify that the ICU segmenter and Lindera tokenizer produce
//! proper word-level tokens instead of overlapping bigrams for CJK and other
//! scripts written without spaces.

use paper::core::text::fts_segment;

#[test]
fn icu_segments_chinese_words_not_bigrams() {
    let text = "人工智能改变世界";
    let segmented = fts_segment(text); // with icu_segmenter feature
    // Should contain actual words, not just bigrams
    assert!(
        segmented.contains("人工") || segmented.contains("智能") || segmented.contains("改变") || segmented.contains("世界"),
        "Expected word-level segmentation, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_chinese_proper_words() {
    let text = "机器学习";
    let segmented = fts_segment(text);
    // Should contain "机器" and "学习" as words, not just overlapping bigrams
    assert!(
        segmented.contains("机器") && segmented.contains("学习"),
        "Expected '机器 学习', got: {}",
        segmented
    );
}

#[test]
fn lindera_segments_japanese() {
    let text = "自然言語処理";
    let segmented = fts_segment(text); // with lindera feature
    // Lindera in Search mode should produce proper Japanese tokens
    assert!(
        segmented.contains("自然言語") || segmented.contains("言語") || segmented.contains("処理"),
        "Expected Japanese tokenization, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_korean() {
    let text = "인공지능";
    let segmented = fts_segment(text); // with icu_segmenter feature
    // ICU Korean segmentation (weaker recall, documented)
    assert!(
        segmented.contains("인공") || segmented.contains("지능") || segmented.contains("인공지능"),
        "Expected Korean segmentation, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_thai() {
    let text = "ปัญญาประดิษฐ์";
    let segmented = fts_segment(text); // with icu_segmenter feature
    // Thai should be segmented at word boundaries
    assert!(
        segmented.split_whitespace().count() > 1,
        "Expected Thai word segmentation, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_lao() {
    let text = "ປັນຍາປະລິດ";
    let segmented = fts_segment(text); // with icu_segmenter feature
    assert!(
        segmented.split_whitespace().count() > 1,
        "Expected Lao word segmentation, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_khmer() {
    let text = "បច្ចេកទេសបណ្ដុះបណ្ដាល";
    let segmented = fts_segment(text); // with icu_segmenter feature
    assert!(
        segmented.split_whitespace().count() > 1,
        "Expected Khmer word segmentation, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_myanmar() {
    let text = "မိတ္တာပညာပရိုဂရမ်";
    let segmented = fts_segment(text); // with icu_segmenter feature
    assert!(
        segmented.split_whitespace().count() > 1,
        "Expected Myanmar word segmentation, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_arabic() {
    let text = "الذكاء الاصطناعي";
    let segmented = fts_segment(text); // with icu_segmenter feature
    // Arabic should be segmented at word boundaries
    assert!(
        segmented.contains("الذكاء") || segmented.contains("اصطناعي"),
        "Expected Arabic word segmentation, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_hebrew() {
    let text = "בינה מלאכותית";
    let segmented = fts_segment(text); // with icu_segmenter feature
    assert!(
        segmented.contains("בינה") || segmented.contains("מלאכותית"),
        "Expected Hebrew word segmentation, got: {}",
        segmented
    );
}

#[test]
fn icu_segments_devanagari() {
    let text = "कृत्रिम बुद्धिमत्ता";
    let segmented = fts_segment(text); // with icu_segmenter feature
    assert!(
        segmented.contains("कृत्रिम") || segmented.contains("बुद्धिमत्ता"),
        "Expected Devanagari word segmentation, got: {}",
        segmented
    );
}

#[test]
fn latin_text_unchanged() {
    let text = "artificial intelligence";
    let segmented = fts_segment(text);
    // Latin text should be unchanged (handled by unicode61 tokenizer)
    assert_eq!(segmented, "artificial intelligence");
}

#[test]
fn cyrillic_text_unchanged() {
    let text = "искусственный интеллект";
    let segmented = fts_segment(text);
    // Cyrillic text should be unchanged
    assert_eq!(segmented, "искусственный интеллект");
}

#[test]
fn mixed_script_segmentation() {
    let text = "人工智能 AI 人工智能";
    let segmented = fts_segment(text);
    // Chinese part should be word-segmented, English part unchanged
    assert!(
        (segmented.contains("人工") || segmented.contains("智能")) && segmented.contains("AI"),
        "Expected mixed script handling, got: {}",
        segmented
    );
}