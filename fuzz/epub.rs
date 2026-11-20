//! EPUB fuzz target using zip + XML fuzzing.
//!
//! Corpus: fuzz-corpus/epub/ (populate with 1000+ malicious EPUB files)
//! Run: cargo fuzz run epub -- -max_total_time=60

#![no_main]

use libfuzzer_sys::fuzz_target;
use quick_xml::Reader;
use std::io::Read;
use zip::ZipArchive;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Try to open as ZIP archive
    if let Ok(mut zip) = ZipArchive::new(Cursor::new(data)) {
        // Check for META-INF/container.xml
        if let Ok(mut file) = zip.by_name("META-INF/container.xml") {
            let mut xml = String::new();
            if file.read_to_string(&mut xml).is_ok() {
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

        // Try reading all xhtml/html files
        for i in 0..zip.len().min(500) {
            if let Ok(mut file) = zip.by_index(i) {
                let fname = file.name().map(|cow| cow.into_owned()).unwrap_or_default();
                if fname.ends_with(".xhtml") || fname.ends_with(".html") || fname.ends_with(".htm") {
                    let mut _xml = String::new();
                    let _ = file.read_to_string(&mut _xml);
                }
            }
        }
    }
});