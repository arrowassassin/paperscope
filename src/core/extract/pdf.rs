use super::{single, Block, ExtractedDocument, Extractor, Capabilities, MAX_PDF_PAGES};
use anyhow::Result;

pub struct Pdf;

impl Extractor for Pdf {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(b"%PDF-")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_file_bytes: 256 * 1024 * 1024, // 256 MB for PDFs
            max_pages: MAX_PDF_PAGES,
            max_entries: 1,
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }

    /// Per-page PDF text via lopdf.
    ///
    /// `extract_text(&[page])` is called one page at a time so a failure on one
    /// page cannot lose the rest of the document, and so the citation anchor is
    /// exact. A page with no text layer is reported as a warning: it may be a
    /// scan, and silently dropping it would make the index look complete when
    /// it is not. OCR is an explicit opt-in elsewhere, never a silent fallback.
    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        let doc = match lopdf::Document::load_mem(bytes) {
            Ok(doc) => doc,
            Err(err) => {
                // One malformed file must not abort the whole index run.
                return Ok(Some(single(
                    name,
                    "pdf",
                    Vec::new(),
                    vec![format!("unreadable PDF: {err}")],
                )));
            }
        };
        let pages = doc.get_pages();
        let mut blocks = Vec::new();
        let mut warnings = Vec::new();
        for (number, _) in pages.iter().take(MAX_PDF_PAGES as usize) {
            let text = match doc.extract_text(&[*number]) {
                Ok(text) => text,
                Err(err) => {
                    warnings.push(format!("page {number}: {err}"));
                    continue;
                }
            };
            if text.trim().is_empty() {
                warnings.push(format!("page {number}: no text layer"));
                continue;
            }
            let lines = text.lines().count().max(1) as i64;
            blocks.push(Block {
                text,
                page: *number as i64,
                anchor: Some(format!("page {number}")),
                section: None,
                line_start: 1,
                line_end: lines,
            });
        }
        Ok(Some(single(name, "pdf", blocks, warnings)))
    }
}
