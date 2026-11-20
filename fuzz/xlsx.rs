//! XLSX fuzz target using zip + calamine fuzzing.
//!
//! Corpus: fuzz-corpus/xlsx/ (populate with 1000+ malicious XLSX files)
//! Run: cargo fuzz run xlsx -- -max_total_time=60

#![no_main]

use libfuzzer_sys::fuzz_target;
use calamine::{open_workbook_auto_from_rs, Reader};
use std::io::Cursor;
use zip::ZipArchive;

fuzz_target!(|data: &[u8]| {
    // Try to open as ZIP archive first
    if let Ok(mut zip) = ZipArchive::new(Cursor::new(data)) {
        // Check for workbook.xml
        if let Ok(mut file) = zip.by_name("xl/workbook.xml") {
            let mut _xml = String::new();
            let _ = std::io::Read::read_to_string(&mut file, &mut _xml);
        }
    }

    // Try to parse with calamine (exercises the full spreadsheet parser)
    if let Ok(mut workbook) = open_workbook_auto_from_rs(Cursor::new(data)) {
        let sheet_names = workbook.sheet_names().to_vec();
        for sheet in sheet_names {
            let _ = workbook.worksheet_range(&sheet);
        }
    }
});