//! PDF fuzz target using lopdf and libfuzzer-sys.
//!
//! Corpus: fuzz-corpus/pdf/ (populate with 1000+ malicious PDFs)
//! Run: cargo fuzz run pdf -- -max_total_time=60

#![no_main]

use libfuzzer_sys::fuzz_target;
use lopdf::Document;

fuzz_target!(|data: &[u8]| {
    // Try to parse as PDF - this exercises lopdf's parser on arbitrary input
    if let Ok(doc) = Document::load_mem(data) {
        // Try to extract text from each page to exercise the full pipeline
        let pages = doc.get_pages();
        for (page_num, _) in pages {
            let _ = doc.extract_text(&[page_num]);
        }
    }
});