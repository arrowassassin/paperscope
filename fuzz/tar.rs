//! TAR fuzz target using tar crate.
//!
//! Corpus: fuzz-corpus/tar/ (populate with 1000+ malicious TAR files)
//! Run: cargo fuzz run tar -- -max_total_time=60

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Read;
use tar::Archive;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Try to open as TAR archive and iterate all entries
    let mut archive = Archive::new(Cursor::new(data));
    if let Ok(entries) = archive.entries() {
        for entry_result in entries {
            if let Ok(mut entry) = entry_result {
                let _ = entry.path();
                let _ = entry.header().size().unwrap_or(0);
                let _ = entry.header().entry_type().is_file();
                let mut _buf = Vec::new();
                let _ = entry.read_to_end(&mut _buf);
            }
        }
    }
});