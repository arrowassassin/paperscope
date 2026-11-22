use crate::core::extract::{single, Block, ExtractedDocument, MAX_EXTRACTED_BYTES};
use anyhow::Result;
use std::path::Path;
use tempfile;

/// OCR engine trait for different platform implementations.
pub trait OcrEngine: Send + Sync {
    /// Recognize text from image bytes.
    fn recognize(&self, image_bytes: &[u8]) -> Result<String>;

    /// Engine name for logging.
    fn name(&self) -> &'static str;
}

/// Tesseract OCR engine (cross-platform, requires tesseract binary).
#[cfg(feature = "ocr")]
pub struct TesseractEngine;

#[cfg(feature = "ocr")]
impl TesseractEngine {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "ocr")]
impl Default for TesseractEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "ocr")]
impl OcrEngine for TesseractEngine {
    fn recognize(&self, image_bytes: &[u8]) -> Result<String> {
        // Write image to a temp file with a random suffix to avoid symlink
        // attacks on predictable names.
        let image_temp = tempfile::Builder::new()
            .prefix("paper_ocr_")
            .suffix(".png")
            .tempfile_in(std::env::temp_dir())?;
        let image_path = image_temp.path().to_path_buf();
        std::fs::write(&image_path, image_bytes)?;

        // Output path prefix: tesseract appends ".txt" itself.
        let output_prefix = image_path.with_extension("");

        // Run tesseract
        let output = std::process::Command::new("tesseract")
            .arg(&image_path)
            .arg(&output_prefix)
            .arg("-l")
            .arg("eng")
            .output()?;

        // Clean up image
        let _ = std::fs::remove_file(&image_path);
        let _ = image_temp.close();

        if !output.status.success() {
            anyhow::bail!(
                "tesseract failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Read output (tesseract appends ".txt" to the prefix).
        let output_path = output_prefix.with_extension("txt");
        let text = std::fs::read_to_string(&output_path)?;
        let _ = std::fs::remove_file(&output_path);

        Ok(text)
    }

    fn name(&self) -> &'static str {
        "tesseract"
    }
}

/// macOS Vision framework OCR engine.
#[cfg(all(feature = "ocr", target_os = "macos"))]
pub struct VisionEngine;

#[cfg(all(feature = "ocr", target_os = "macos"))]
impl VisionEngine {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(all(feature = "ocr", target_os = "macos"))]
impl Default for VisionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "ocr", target_os = "macos"))]
impl OcrEngine for VisionEngine {
    fn recognize(&self, image_bytes: &[u8]) -> Result<String> {
        // Use the vision crate for macOS Vision framework OCR
        // This is a placeholder - actual implementation would use the vision crate
        // For now, fall back to tesseract if available
        let tesseract = TesseractEngine::new();
        tesseract.recognize(image_bytes)
    }

    fn name(&self) -> &'static str {
        "macos-vision"
    }
}

/// Windows OCR engine.
#[cfg(all(feature = "ocr", target_os = "windows"))]
pub struct WindowsOcrEngine;

#[cfg(all(feature = "ocr", target_os = "windows"))]
impl WindowsOcrEngine {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(all(feature = "ocr", target_os = "windows"))]
impl Default for WindowsOcrEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "ocr", target_os = "windows"))]
impl OcrEngine for WindowsOcrEngine {
    fn recognize(&self, image_bytes: &[u8]) -> Result<String> {
        // Use Windows OCR API
        // Placeholder - would use windows-ocr crate
        let tesseract = TesseractEngine::new();
        tesseract.recognize(image_bytes)
    }

    fn name(&self) -> &'static str {
        "windows-ocr"
    }
}

/// Detect and return the best available OCR engine.
///
/// All `recognize()` implementations currently fall back to the `tesseract`
/// binary, so verify it is actually available before advertising any engine;
/// otherwise return `None` and let callers handle the absence gracefully
/// (M16).
#[cfg(feature = "ocr")]
pub fn detect_ocr_engine() -> Option<Box<dyn OcrEngine>> {
    if which::which("tesseract").is_err() {
        return None;
    }

    #[cfg(target_os = "macos")]
    {
        Some(Box::new(VisionEngine::new()))
    }
    #[cfg(target_os = "windows")]
    {
        Some(Box::new(WindowsOcrEngine::new()))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Some(Box::new(TesseractEngine::new()))
    }
}

/// Process a PDF page with OCR if it has no text layer.
///
/// This is called from the PDF extractor when a page has no extractable text.
#[cfg(feature = "ocr")]
pub fn process_pdf_page_with_ocr(
    page_num: u32,
    pdf_path: &Path,
) -> Result<Option<Vec<ExtractedDocument>>> {
    let engine = detect_ocr_engine();
    let Some(engine) = engine else {
        tracing::warn!(
            "page {}: no OCR engine available (install tesseract or enable platform OCR)",
            page_num
        );
        return Ok(None);
    };

    // Render PDF page to image using pdftoppm or similar
    let temp_dir = std::env::temp_dir();
    let image_path = temp_dir.join(format!("paper_ocr_{}_{}.png", std::process::id(), page_num));

    // Use pdftoppm to render the page
    let output = std::process::Command::new("pdftoppm")
        .arg("-png")
        .arg("-f")
        .arg(page_num.to_string())
        .arg("-l")
        .arg(page_num.to_string())
        .arg("-singlefile")
        .arg("-")
        .arg(image_path.with_extension(""))
        .stdin(std::fs::File::open(pdf_path)?)
        .output()?;

    if !output.status.success() {
        tracing::warn!(
            "page {}: failed to render for OCR: {}",
            page_num,
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(None);
    }

    // Read the rendered image
    let image_bytes = std::fs::read(&image_path)?;
    let _ = std::fs::remove_file(&image_path);

    // Run OCR
    let text = match engine.recognize(&image_bytes) {
        Ok(text) => text,
        Err(e) => {
            tracing::warn!("page {}: OCR failed: {}", page_num, e);
            return Ok(None);
        }
    };

    if text.trim().is_empty() {
        tracing::warn!("page {}: OCR produced no text", page_num);
        return Ok(None);
    }

    // Truncate if too large
    let text = if text.len() > MAX_EXTRACTED_BYTES {
        let mut cut = MAX_EXTRACTED_BYTES;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text[..cut].to_string()
    } else {
        text
    };

    tracing::warn!(
        "page {}: OCR applied using {} (no text layer)",
        page_num,
        engine.name()
    );

    let line_end = text.lines().count().max(1) as i64;
    Ok(Some(single(
        &pdf_path.to_string_lossy(),
        "pdf",
        vec![Block {
            text,
            page: page_num as i64,
            anchor: Some(format!("page {} (OCR)", page_num)),
            section: Some("OCR".to_string()),
            line_start: 1,
            line_end,
        }],
        vec![format!(
            "page {}: OCR applied using {}",
            page_num,
            engine.name()
        )],
    )))
}

#[cfg(not(feature = "ocr"))]
pub fn detect_ocr_engine() -> Option<Box<dyn OcrEngine>> {
    None
}

#[cfg(not(feature = "ocr"))]
pub fn process_pdf_page_with_ocr(
    _page_num: u32,
    _pdf_path: &Path,
) -> Result<Option<Vec<ExtractedDocument>>> {
    Ok(None)
}
