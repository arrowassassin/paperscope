//! Document extraction with media-native citation anchors.
//!
//! # Design
//!
//! Extraction produces **facts**, never interpretations. A block carries the
//! text exactly as the file states it plus the locator the file itself exposes:
//! a PDF page number, a spreadsheet cell range, a slide index, a paragraph
//! range, an EPUB spine position, an email message id. Nothing is classified,
//! dated, summarised or normalised semantically. If the library cannot read a
//! format or a page, it says so; it never guesses.
//!
//! Formats are handled by an ordered registry of [`Extractor`] implementations.
//! Selection is by declared extension first and by unambiguous magic bytes
//! second; an unrecognised file is *unsupported*, not "probably text". New
//! formats are additive: implement `Extractor` and [`register`] it.
//!
//! # Anchors
//!
//! A chunk inherits the anchor of the block it was cut from, so a citation can
//! name `report.pdf [page 7]`, `budget.xlsx [Q3!B4:H4]` or `deck.pptx [slide 2]`
//! and a reader can verify it in one step. Line ranges are added where the
//! format has real lines.

mod accel;
mod archive;
mod email;
mod ocr;
mod ooxml;
mod pdf;
mod sheet;
mod textfmt;

use anyhow::Result;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// Hard cap on a single extracted text, to bound memory on hostile input.
pub const MAX_EXTRACTED_BYTES: usize = 32 * 1024 * 1024;
/// Hard cap on archive members and PDF pages.
pub const MAX_ARCHIVE_ENTRIES: usize = 2_000;
pub const MAX_PDF_PAGES: u32 = 5_000;

/// Capabilities declared by an extractor.
///
/// These bounds are enforced by the extraction pipeline before and during
/// parsing. An extractor should choose conservative values that reflect the
/// worst-case resource consumption of its parser on malicious input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Maximum file size this extractor will accept, in bytes.
    /// Files larger than this are rejected before parsing begins.
    pub max_file_bytes: u64,
    /// Maximum number of pages (for paged formats) or entries (for archives).
    pub max_pages: u32,
    /// Maximum number of archive members or spreadsheet rows to process.
    pub max_entries: usize,
    /// Whether the extractor can process input incrementally without loading
    /// the entire file into memory.
    pub streaming: bool,
    /// Whether the extractor is safe to call concurrently from multiple tasks.
    pub thread_safe: bool,
    /// Whether the extractor requires an external binary or system dependency.
    pub requires_external: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024, // 64 MB
            max_pages: 5_000,
            max_entries: 2_000,
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }
}

/// One extracted document. Archives yield several, addressed as
/// `outer.zip!/inner/path`.
#[derive(Debug, Clone)]
pub struct ExtractedDocument {
    pub path: String,
    pub format: &'static str,
    pub blocks: Vec<Block>,
    /// Explicit failures such as `page 3: no text layer`. Reported to the user
    /// instead of being silently swallowed.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub text: String,
    /// 1-based page for paged formats, 0 when the format has no pages.
    pub page: i64,
    /// Format-specific anchor: `page 3`, `Sheet1!A5:H5`, `slide 2`,
    /// `para 4-9`, `spine 1: chapter.xhtml`, `message <id>`.
    pub anchor: Option<String>,
    /// Declared section path (`Warranty / Coverage`), when the format states
    /// one. Never inferred from layout.
    pub section: Option<String>,
    /// 1-based line range *within the block's text source*, 0 when the format
    /// has no meaningful lines (DOCX, spreadsheets, slides, email).
    pub line_start: i64,
    pub line_end: i64,
}

impl Block {
    pub fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        let lines = text.lines().count().max(1) as i64;
        Self {
            text,
            page: 0,
            anchor: None,
            section: None,
            line_start: 1,
            line_end: lines,
        }
    }

    pub fn anchored(text: impl Into<String>, anchor: impl Into<String>) -> Self {
        let mut block = Self::text(text);
        block.anchor = Some(anchor.into());
        block
    }
}

/// A format handler.
///
/// `extract` must either return text it actually decoded or `Ok(None)`. It must
/// not fall back to treating arbitrary bytes as text.
pub trait Extractor: Send + Sync {
    /// Stable format name, used for reporting and for `Hit::anchor_kind`.
    fn name(&self) -> &'static str;

    /// Extensions handled, lower-case, without the dot.
    fn extensions(&self) -> &'static [&'static str];

    /// Unambiguous content check, for files with a missing or wrong extension.
    fn sniff(&self, _bytes: &[u8]) -> bool {
        false
    }

    /// Resource and safety capabilities of this extractor.
    ///
    /// The extraction pipeline enforces `max_file_bytes` before calling
    /// `extract`, wraps the call in a 5-second timeout, and respects
    /// `thread_safe` when deciding on parallelism.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn can_handle(&self, name: &str, bytes: &[u8]) -> bool {
        self.extensions().contains(&extension_of(name).as_str()) || self.sniff(bytes)
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>>;
}

fn builtin() -> Vec<Arc<dyn Extractor>> {
    vec![
        Arc::new(pdf::Pdf),
        Arc::new(ooxml::Docx),
        Arc::new(ooxml::Pptx),
        Arc::new(ooxml::Epub),
        Arc::new(sheet::Sheet),
        Arc::new(archive::Archive),
        Arc::new(email::Email),
        Arc::new(textfmt::Html),
        // Last: only claims extensions it declares or unambiguous text markers.
        Arc::new(textfmt::Plain),
    ]
}

fn custom() -> &'static Mutex<Vec<Arc<dyn Extractor>>> {
    static CUSTOM: OnceLock<Mutex<Vec<Arc<dyn Extractor>>>> = OnceLock::new();
    CUSTOM.get_or_init(|| Mutex::new(Vec::new()))
}

/// Add an extractor. Custom extractors are tried after the built-ins, so a
/// custom handler can widen support without changing built-in behaviour.
pub fn register(extractor: Arc<dyn Extractor>) {
    if let Ok(mut list) = custom().lock() {
        list.push(extractor);
    }
}

/// Every registered extractor, built-ins first.
pub fn extractors() -> Vec<Arc<dyn Extractor>> {
    let mut all = builtin();
    if let Ok(list) = custom().lock() {
        all.extend(list.iter().cloned());
    }
    all
}

/// Every extension any extractor declares.
pub fn supported_extensions() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for extractor in extractors() {
        out.extend_from_slice(extractor.extensions());
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub fn extension_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Whether any extractor claims this path by extension.
pub fn is_supported(path: &str) -> bool {
    let ext = extension_of(path);
    extractors()
        .iter()
        .any(|e| e.extensions().contains(&ext.as_str()))
}

/// Extract a file from disk. Returns zero or more documents (archives yield
/// many). An unsupported file is `Ok(None)`.
pub fn extract_path(path: &Path) -> Result<Option<Vec<ExtractedDocument>>> {
    let name = path.to_string_lossy().to_string();
    if !is_supported(&name) {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    extract_named(&name, &bytes, false)
}

/// Extract bytes that are already in memory (archive members, tests).
/// `ocr_enabled` controls whether OCR is attempted for scanned PDF pages.
pub fn extract_named(name: &str, bytes: &[u8], ocr_enabled: bool) -> Result<Option<Vec<ExtractedDocument>>> {
    // Try shell-out accelerators first (feature-gated)
    #[cfg(feature = "shell-accel")]
    {
        if let Some(path) = std::path::Path::new(name).extension() {
            if let Some(ext) = path.to_str() {
                if let Ok(Some(docs)) = crate::core::extract::accel::try_accelerators(
                    std::path::Path::new(name),
                    ext,
                ) {
                    if !docs.is_empty() {
                        return Ok(Some(docs));
                    }
                }
            }
        }
    }

    for extractor in extractors() {
        if !extractor.can_handle(name, bytes) {
            continue;
        }
        let caps = extractor.capabilities();

        // Enforce memory cap before parsing.
        if bytes.len() as u64 > caps.max_file_bytes {
            return Ok(Some(single(
                name,
                extractor.name(),
                Vec::new(),
                vec![format!(
                    "file size {} exceeds maximum {} bytes",
                    bytes.len(),
                    caps.max_file_bytes
                )],
            )));
        }

        // Per-parser timeout: run extract in a separate thread with a 5-second deadline.
        // Using std::thread + mpsc avoids tokio runtime nesting issues.
        let extractor_clone = extractor.clone();
        let name_owned = name.to_string();
        let bytes_owned = bytes.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(extractor_clone.extract(&name_owned, &bytes_owned));
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(5));

        let mut docs = match result {
            Ok(Ok(Some(docs))) => docs,
            Ok(Ok(None)) => continue,
            Ok(Err(err)) => {
                return Ok(Some(single(
                    name,
                    extractor.name(),
                    Vec::new(),
                    vec![format!("extraction error: {err}")],
                )));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                return Ok(Some(single(
                    name,
                    extractor.name(),
                    Vec::new(),
                    vec!["extraction timed out after 5 seconds".to_string()],
                )));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Ok(Some(single(
                    name,
                    extractor.name(),
                    Vec::new(),
                    vec!["extraction task panicked".to_string()],
                )));
            }
        };

        for doc in &mut docs {
            doc.format = extractor.name();
            if doc.path.is_empty() {
                doc.path = name.to_string();
            }
            doc.blocks = finish_blocks(std::mem::take(&mut doc.blocks));
        }
        docs.retain(|doc| !doc.blocks.is_empty());
        if docs.is_empty() {
            // Matched the format but had nothing citable (empty or unreadable).
            return Ok(None);
        }
        return Ok(Some(docs));
    }
    Ok(None)
}

/// Drop empty blocks and cap their size. Anchors are never modified.
pub(crate) fn finish_blocks(mut blocks: Vec<Block>) -> Vec<Block> {
    let mut total = 0usize;
    blocks.retain(|b| !b.text.trim().is_empty());
    for block in &mut blocks {
        if block.text.len() > MAX_EXTRACTED_BYTES {
            let mut cut = MAX_EXTRACTED_BYTES;
            while cut > 0 && !block.text.is_char_boundary(cut) {
                cut -= 1;
            }
            block.text.truncate(cut);
        }
        total += block.text.len();
        if total > MAX_EXTRACTED_BYTES {
            break;
        }
    }
    blocks
}

/// Helper for extractors: build a single-document result.
pub(crate) fn single(
    name: &str,
    format: &'static str,
    blocks: Vec<Block>,
    warnings: Vec<String>,
) -> Vec<ExtractedDocument> {
    vec![ExtractedDocument {
        path: name.to_string(),
        format,
        blocks,
        warnings,
    }]
}