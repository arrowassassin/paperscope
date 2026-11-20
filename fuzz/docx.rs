//! DOCX fuzz target using zip + XML fuzzing.
//!
//! Corpus: fuzz-corpus/docx/ (populate with 1000+ malicious DOCX files)
//! Run: cargo fuzz run docx -- -max_total_time=60

#![no_main]

use libfuzzer_sys::fuzz_target;
use quick_xml::Reader;
use std::io::Read;
use zip::ZipArchive;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Try to open as ZIP archive
    if let Ok(mut zip) = ZipArchive::new(Cursor::new(data)) {
        // Try to read word/document.xml
        if let Ok(mut file) = zip.by_name("word/document.xml") {
            let mut xml = String::new();
            if file.read_to_string(&mut xml).is_ok() {
                // Parse the XML to exercise quick_xml
                let mut reader = Reader::from_str(&xml);
                let mut buf = Vec::new();
                loop {
                    match reader.read_event_into(&mut buf) {
                        Ok(quick_xml::events::Event::Eof) => break,
                        Ok(_) => {}
                        Err(_) => break,
                    }
                    buf.clear();
                }
            }
        }
        // Also try reading other common DOCX entries
        for i in 0..zip.len().min(100) {
            if let Ok(mut file) = zip.by_index(i) {
                let _ = file.name();
                let mut _buf = Vec::new();
                let _ = file.read_to_end(&mut _buf);
            }
        }
    }
});