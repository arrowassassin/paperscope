//! ZIP fuzz target using zip crate.
//!
//! Corpus: fuzz-corpus/zip/ (populate with 1000+ malicious ZIP files)
//! Run: cargo fuzz run zip -- -max_total_time=60

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Read;
use zip::ZipArchive;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Try to open as ZIP archive and iterate all entries
    if let Ok(mut zip) = ZipArchive::new(Cursor::new(data)) {
        for i in 0..zip.len() {
            if let Ok(mut file) = zip.by_index(i) {
                let _ = file.name();
                let _ = file.size();
                let _ = file.compressed_size();
                let _ = file.is_dir();
                let mut _buf = Vec::new();
                let _ = file.read_to_end(&mut _buf);
            }
        }
    }
});