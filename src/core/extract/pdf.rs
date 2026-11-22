use super::{
    single, Block, Capabilities, ExtractOptions, ExtractedDocument, Extractor, MAX_PDF_PAGES,
};
use anyhow::Result;

pub struct Pdf;

/// A page whose text layer has fewer non-whitespace characters than this is
/// treated as scanned. Real text pages clear it by a wide margin; scans return
/// either nothing or a handful of stray glyphs from an OCR'd header.
const TEXT_LAYER_MIN_CHARS: usize = 32;

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
    /// it is not.
    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        self.extract_with_options(name, bytes, &ExtractOptions::default())
    }

    /// Same as [`Pdf::extract`], plus an OCR pass over textless pages.
    ///
    /// The page bitmap is pulled straight out of the PDF's own object graph, so
    /// no rasterizer (poppler, MuPDF, pdfium) is needed for ordinary scans:
    /// a scanned page in practice carries one full-page `DCTDecode` (JPEG) or
    /// `FlateDecode` image. Pages whose image is in a format we cannot decode
    /// are warned about rather than silently skipped.
    fn extract_with_options(
        &self,
        name: &str,
        bytes: &[u8],
        options: &ExtractOptions,
    ) -> Result<Option<Vec<ExtractedDocument>>> {
        #[cfg(not(feature = "ocr"))]
        let _ = options;
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
        for (number, page_id) in pages.iter().take(MAX_PDF_PAGES as usize) {
            #[cfg(not(feature = "ocr"))]
            let _ = page_id;
            let text = match doc.extract_text(&[*number]) {
                Ok(text) => text,
                Err(err) => {
                    warnings.push(format!("page {number}: {err}"));
                    continue;
                }
            };
            let citable = text.chars().filter(|c| !c.is_whitespace()).count();
            if citable >= TEXT_LAYER_MIN_CHARS {
                push_text_block(&mut blocks, *number, text);
                continue;
            }

            #[cfg(feature = "ocr")]
            if options.ocr_enabled {
                match ocr_page(&doc, *page_id, *number) {
                    Ok(Some(block)) => {
                        // Keep any stray real text alongside the OCR text so a
                        // partially scanned page does not lose its text layer.
                        if citable > 0 {
                            push_text_block(&mut blocks, *number, text);
                        }
                        blocks.push(block);
                        continue;
                    }
                    Ok(None) => {
                        warnings.push(format!(
                            "page {number}: no text layer and no decodable image"
                        ));
                        continue;
                    }
                    Err(err) => {
                        warnings.push(format!("page {number}: OCR failed: {err}"));
                        continue;
                    }
                }
            }

            if citable > 0 {
                push_text_block(&mut blocks, *number, text);
            } else {
                warnings.push(format!("page {number}: no text layer"));
            }
        }
        Ok(Some(single(name, "pdf", blocks, warnings)))
    }
}

fn push_text_block(blocks: &mut Vec<Block>, number: u32, text: String) {
    let lines = text.lines().count().max(1) as i64;
    blocks.push(Block {
        text,
        page: number as i64,
        anchor: Some(format!("page {number}")),
        section: None,
        line_start: 1,
        line_end: lines,
    });
}

/// Render one textless page through OCR and return it as a citable block.
///
/// `Ok(None)` means the page has no image we can decode — a vector-only page,
/// or a scan compressed with CCITT/JBIG2/JPX. That is reported upstream as a
/// warning, never as a silent success.
#[cfg(feature = "ocr")]
fn ocr_page(doc: &lopdf::Document, page_id: lopdf::ObjectId, number: u32) -> Result<Option<Block>> {
    let images = doc.get_page_images(page_id).unwrap_or_default();
    // A scan is one large bitmap; a letterhead or logo may also be an image.
    // The largest image on the page is the least surprising choice.
    let Some(best) = images
        .iter()
        .filter(|img| img.width >= 64 && img.height >= 64)
        .max_by_key(|img| img.width.saturating_mul(img.height))
    else {
        return Ok(None);
    };
    let width = best.width as u32;
    let height = best.height as u32;
    let filters: Vec<&str> = best
        .filters
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(String::as_str)
        .collect();

    let decoded = if filters.contains(&"DCTDecode") {
        // JPEG bytes are already a complete image; hand them to the decoder.
        image::load_from_memory(best.content).ok()
    } else if filters.is_empty() || filters.iter().all(|f| *f == "FlateDecode") {
        let stream = doc.get_object(best.id)?.as_stream()?;
        let raw = stream.decompressed_content()?;
        raw_pixels_to_image(
            &raw,
            width,
            height,
            best.color_space.as_deref(),
            best.bits_per_component.unwrap_or(8),
        )
    } else {
        None
    };

    let Some(image) = decoded else {
        return Ok(None);
    };
    let text = super::ocr::recognize_image(&image)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let lines = text.lines().count().max(1) as i64;
    Ok(Some(Block {
        text,
        page: number as i64,
        anchor: Some(format!("page {number} (OCR)")),
        section: Some("OCR".to_string()),
        line_start: 1,
        line_end: lines,
    }))
}

/// Turn a decompressed PDF image stream into an image we can OCR.
///
/// Only the two layouts scanners actually emit are supported: 8-bit
/// `DeviceRGB` (3 bytes per pixel) and 8-bit `DeviceGray` (1 byte per pixel).
/// Anything else returns `None` so the caller can warn instead of guessing.
#[cfg(feature = "ocr")]
fn raw_pixels_to_image(
    raw: &[u8],
    width: u32,
    height: u32,
    color_space: Option<&str>,
    bits_per_component: i64,
) -> Option<image::DynamicImage> {
    if bits_per_component != 8 {
        return None;
    }
    let pixels = width as usize * height as usize;
    match color_space {
        Some("DeviceRGB") | Some("RGB") | None if raw.len() >= pixels * 3 => {
            let rgb = image::RgbImage::from_raw(width, height, raw[..pixels * 3].to_vec())?;
            Some(image::DynamicImage::ImageRgb8(rgb))
        }
        Some("DeviceGray") | Some("G") if raw.len() >= pixels => {
            let gray = image::GrayImage::from_raw(width, height, raw[..pixels].to_vec())?;
            Some(image::DynamicImage::ImageLuma8(gray))
        }
        _ => None,
    }
}
