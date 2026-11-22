//! OCR for scanned pages and standalone images.
//!
//! Engine: PaddleOCR PP-OCRv4 detection + recognition ONNX models
//! (Apache-2.0), executed by `tract-onnx` — pure Rust, no C/C++ runtime and no
//! system binaries. The ~16 MB of weights and the character dictionary are
//! downloaded on first use, exactly like the embedding models.
//!
//! Why not Tesseract: it needs a C++ toolchain and a system library, which
//! breaks the static musl build and the "one binary, no dependencies" promise.
//! Why not `ocrs`: its weights inherit CC BY-SA 4.0 from HierText, which is
//! copyleft and cannot be redistributed or auto-downloaded by this project.
//!
//! OCR is best-effort by design: a missing model, an unreadable page or a
//! failed decode downgrades to "no text" and never fails an index run.

use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Context, Result};
use image::{DynamicImage, GenericImageView, RgbImage};
use sha2::{Digest, Sha256};
use tract_onnx::prelude::*;

use super::{single, Block, ExtractedDocument, Extractor, MAX_EXTRACTED_BYTES};

pub const DET_FILE: &str = "det.onnx";
pub const REC_FILE: &str = "rec.onnx";
pub const DICT_FILE: &str = "ppocr_keys_v1.txt";

/// Detection input is a fixed square canvas; a single shape keeps `tract`
/// fast and lets us cache one optimised plan for the whole process.
const DET_SIDE: usize = 960;
const REC_HEIGHT: u32 = 48;
const REC_WIDTH: u32 = 960;
/// Longest edge kept from a source image before OCR (memory/latency cap).
const MAX_LONG_EDGE: u32 = 2000;
const DET_PIXEL_THRESHOLD: f32 = 0.3;
const DET_BOX_SCORE: f32 = 0.5;
const DET_MIN_AREA: u32 = 6;
/// DB outputs shrink by ~2x, so expand each box before cropping.
const DET_UNCLIP_RATIO: f32 = 1.6;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// One recognised line of text.
pub struct OcrLine {
    pub text: String,
    pub confidence: f32,
}

/// Where the OCR weights live: `$PAPER_HOME/models/paddleocr`.
pub fn model_dir() -> PathBuf {
    crate::core::paths::Paths::new()
        .model_cache_dir()
        .join("paddleocr")
}

/// True when every model file is on disk.
pub fn models_present() -> bool {
    let dir = model_dir();
    dir.join(DET_FILE).is_file() && dir.join(REC_FILE).is_file() && dir.join(DICT_FILE).is_file()
}

/// OCR is on by default when the weights are here; `PAPER_NO_OCR=1` wins.
pub fn available() -> bool {
    if std::env::var("PAPER_NO_OCR").is_ok_and(|v| !v.is_empty() && v != "0") {
        return false;
    }
    models_present()
}

/// Resolve whether a command should run OCR.
pub fn enabled(explicit: bool) -> bool {
    if std::env::var("PAPER_NO_OCR").is_ok_and(|v| !v.is_empty() && v != "0") {
        return false;
    }
    explicit || models_present()
}

/// Recognise text in encoded image bytes (PNG, JPEG, TIFF, WebP, BMP, GIF).
pub fn recognize_bytes(bytes: &[u8]) -> Result<String> {
    let image = image::load_from_memory(bytes).context("decode image for OCR")?;
    recognize_image(&image)
}

/// Recognise text in an already-decoded image.
pub fn recognize_image(image: &DynamicImage) -> Result<String> {
    let image = lift_small_image(&clamp_long_edge(image, MAX_LONG_EDGE), 1600);
    let engine = engine()?;
    let lines = engine.recognize(&image)?;
    let mut text = lines
        .into_iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if text.len() > MAX_EXTRACTED_BYTES {
        let mut cut = MAX_EXTRACTED_BYTES;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
    Ok(text)
}

/// Bring very small images up to a size the detector can read.
///
/// A PDF page image saved at 72 dpi is often under 1000 px on the long edge,
/// where thin letters and word gaps collapse. Upscaling costs little and
/// measurably improves spacing; it never pretends to recover missing detail.
fn lift_small_image(image: &DynamicImage, min_edge: u32) -> DynamicImage {
    let (w, h) = image.dimensions();
    let long = w.max(h);
    if long == 0 || long >= min_edge {
        return image.clone();
    }
    let factor = (min_edge as f32 / long as f32).ceil().clamp(1.0, 3.0);
    let nw = (w as f32 * factor).round().max(1.0) as u32;
    let nh = (h as f32 * factor).round().max(1.0) as u32;
    image.resize(nw, nh, image::imageops::FilterType::Lanczos3)
}

fn clamp_long_edge(image: &DynamicImage, max_edge: u32) -> DynamicImage {
    let (w, h) = image.dimensions();
    let long = w.max(h);
    if long <= max_edge || long == 0 {
        return image.clone();
    }
    let scale = max_edge as f32 / long as f32;
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    image.resize(nw, nh, image::imageops::FilterType::Triangle)
}

static ENGINE: OnceLock<Result<PaddleEngine, String>> = OnceLock::new();

fn engine() -> Result<&'static PaddleEngine> {
    match ENGINE.get_or_init(|| PaddleEngine::load().map_err(|err| format!("{err:#}"))) {
        Ok(engine) => Ok(engine),
        Err(message) => Err(anyhow!("OCR engine unavailable: {message}")),
    }
}

type Plan = std::sync::Arc<RunnableModel<TypedFact, Box<dyn TypedOp>>>;

struct PaddleEngine {
    det: Plan,
    rec: Plan,
    dict: Vec<String>,
}

impl PaddleEngine {
    fn load() -> Result<Self> {
        let dir = model_dir();
        let det = load_plan(&dir.join(DET_FILE), [1, 3, DET_SIDE, DET_SIDE])
            .context("load text-detection model")?;
        let rec = load_plan(
            &dir.join(REC_FILE),
            [1, 3, REC_HEIGHT as usize, REC_WIDTH as usize],
        )
        .context("load text-recognition model")?;
        let raw = std::fs::read_to_string(dir.join(DICT_FILE)).context("read OCR dictionary")?;
        // PaddleOCR's CTC classes are `blank + dictionary + space`.
        let mut dict = raw.lines().map(|line| line.to_string()).collect::<Vec<_>>();
        dict.push(" ".to_string());
        Ok(Self { det, rec, dict })
    }
}

fn load_plan(path: &std::path::Path, shape: [usize; 4]) -> Result<Plan> {
    if !path.is_file() {
        bail!("missing model file {}", path.display());
    }
    let plan = tract_onnx::onnx()
        .model_for_path(path)?
        .with_input_fact(0, f32::fact(shape).into())?
        .into_optimized()?
        .into_runnable()?;
    Ok(plan)
}

impl PaddleEngine {
    fn recognize(&self, image: &DynamicImage) -> Result<Vec<OcrLine>> {
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Ok(Vec::new());
        }

        // --- detection on a letterboxed 960x960 canvas -------------------
        let scale = (DET_SIDE as f32 / width as f32).min(DET_SIDE as f32 / height as f32);
        let scaled_w = ((width as f32 * scale).round() as u32).max(1);
        let scaled_h = ((height as f32 * scale).round() as u32).max(1);
        let resized = image
            .resize_exact(scaled_w, scaled_h, image::imageops::FilterType::Triangle)
            .to_rgb8();
        let pad_x = (DET_SIDE as u32 - scaled_w) / 2;
        let pad_y = (DET_SIDE as u32 - scaled_h) / 2;

        let mut canvas = vec![1.0f32; 3 * DET_SIDE * DET_SIDE];
        let plane = DET_SIDE * DET_SIDE;
        for (x, y, px) in resized.enumerate_pixels() {
            let cx = pad_x as usize + x as usize;
            let cy = pad_y as usize + y as usize;
            let idx = cy * DET_SIDE + cx;
            for c in 0..3 {
                let value = px.0[c] as f32 / 255.0;
                canvas[c * plane + idx] = (value - MEAN[c]) / STD[c];
            }
        }
        let input = Tensor::from_shape(&[1, 3, DET_SIDE, DET_SIDE], &canvas)?;
        let outputs = self.det.run(tvec!(input.into()))?;
        let probability = output_view(&outputs[0])?;
        if probability.shape()[2] != DET_SIDE || probability.shape()[3] != DET_SIDE {
            bail!(
                "unexpected detection output shape {:?}",
                probability.shape()
            );
        }

        let boxes = detect_boxes(&probability);
        if boxes.is_empty() {
            return Ok(Vec::new());
        }

        let rgb = image.to_rgb8();
        let mut lines = Vec::new();
        for b in boxes {
            // Map the box back through the letterbox transform.
            let left = ((b.x0 as f32 - pad_x as f32) / scale).max(0.0);
            let top = ((b.y0 as f32 - pad_y as f32) / scale).max(0.0);
            let right = ((b.x1 as f32 - pad_x as f32) / scale).min(width as f32);
            let bottom = ((b.y1 as f32 - pad_y as f32) / scale).min(height as f32);
            let x = left.round() as u32;
            let y = top.round() as u32;
            let w = (right - left).round().max(1.0) as u32;
            let h = (bottom - top).round().max(1.0) as u32;
            if w < 4 || h < 4 || x >= width || y >= height {
                continue;
            }
            let w = w.min(width - x);
            let h = h.min(height - y);
            let crop = image::imageops::crop_imm(&rgb, x, y, w, h).to_image();
            let (text, confidence) = self.recognize_line(&crop)?;
            if !text.trim().is_empty() {
                lines.push(OcrLine { text, confidence });
            }
        }
        Ok(lines)
    }

    fn recognize_line(&self, crop: &RgbImage) -> Result<(String, f32)> {
        let (cw, ch) = crop.dimensions();
        let ratio = cw as f32 / ch.max(1) as f32;
        // Preserve the aspect ratio and pad the canvas, never squash it: a long
        // line forced into a narrow width loses spaces and thin letters, which
        // is exactly the damage that makes OCR output uncitable.
        let wanted_w = (REC_HEIGHT as f32 * ratio).round().max(8.0) as u32;
        let (rw, rh) = if wanted_w <= REC_WIDTH {
            (wanted_w.max(1), REC_HEIGHT)
        } else {
            let scaled_h = ((REC_HEIGHT as f32 * REC_WIDTH as f32 / wanted_w as f32).round()
                as u32)
                .clamp(1, REC_HEIGHT);
            (REC_WIDTH, scaled_h)
        };
        let resized = image::imageops::resize(crop, rw, rh, image::imageops::FilterType::Triangle);

        let plane = (REC_HEIGHT as usize) * (REC_WIDTH as usize);
        // Padding is a black pixel after normalisation: (0/255 - 0.5) / 0.5.
        let mut data = vec![-1.0f32; 3 * plane];
        for (x, y, px) in resized.enumerate_pixels() {
            let idx = y as usize * REC_WIDTH as usize + x as usize;
            for c in 0..3 {
                let value = px.0[c] as f32 / 255.0;
                data[c * plane + idx] = (value - 0.5) / 0.5;
            }
        }
        let input = Tensor::from_shape(&[1, 3, REC_HEIGHT as usize, REC_WIDTH as usize], &data)?;
        let outputs = self.rec.run(tvec!(input.into()))?;
        let probabilities = output_view(&outputs[0])?;
        let shape = probabilities.shape();
        let (steps, classes) = (shape[1], shape[2]);

        let mut text = String::new();
        let mut score_sum = 0.0f32;
        let mut score_n = 0usize;
        let mut previous = usize::MAX;
        for step in 0..steps {
            let mut best = 0usize;
            let mut best_p = 0.0f32;
            for class in 0..classes {
                let p = probabilities[[0, step, class]];
                if p > best_p {
                    best_p = p;
                    best = class;
                }
            }
            if best == 0 || best == previous {
                previous = best;
                continue;
            }
            previous = best;
            if let Some(ch) = self.dict.get(best - 1) {
                text.push_str(ch);
                score_sum += best_p;
                score_n += 1;
            }
        }
        let confidence = if score_n == 0 {
            0.0
        } else {
            score_sum / score_n as f32
        };
        Ok((text, confidence))
    }
}

struct DecodedBox {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

/// Connected components over the detection probability map, converted to
/// reading-ordered boxes. This is the DB post-process, simplified: rectangles
/// are close enough for document scans and avoid a polygon-offset dependency.
/// Borrow a run output as an ndarray view for shape-aware indexing.
fn output_view<'a>(value: &'a TValue) -> Result<tract_ndarray::ArrayViewD<'a, f32>> {
    let view = value.view();
    let slice = view.as_slice::<f32>()?;
    Ok(tract_ndarray::ArrayViewD::from_shape(view.shape(), slice)?)
}

fn detect_boxes(probability: &tract_ndarray::ArrayViewD<f32>) -> Vec<DecodedBox> {
    const SIDE: usize = DET_SIDE;
    let mut mask = vec![false; SIDE * SIDE];
    for y in 0..SIDE {
        for x in 0..SIDE {
            mask[y * SIDE + x] = probability[[0, 0, y, x]] >= DET_PIXEL_THRESHOLD;
        }
    }

    let mut labels = vec![0u32; SIDE * SIDE];
    let mut boxes: Vec<(i32, i32, i32, i32, u32, f32)> = Vec::new();
    let mut next_label = 1u32;
    let mut stack = Vec::new();

    for start in 0..SIDE * SIDE {
        if !mask[start] || labels[start] != 0 {
            continue;
        }
        let label = next_label;
        next_label += 1;
        stack.clear();
        stack.push(start);
        labels[start] = label;

        let (mut min_x, mut min_y, mut max_x, mut max_y) = (SIDE as i32, SIDE as i32, -1i32, -1i32);
        let mut area = 0u32;
        let mut score = 0.0f32;

        while let Some(index) = stack.pop() {
            let y = index / SIDE;
            let x = index % SIDE;
            area += 1;
            score += probability[[0, 0, y, x]];
            min_x = min_x.min(x as i32);
            max_x = max_x.max(x as i32);
            min_y = min_y.min(y as i32);
            max_y = max_y.max(y as i32);

            let neighbours = [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
                (x.wrapping_sub(1), y.wrapping_sub(1)),
                (x + 1, y.wrapping_sub(1)),
                (x.wrapping_sub(1), y + 1),
                (x + 1, y + 1),
            ];
            for (nx, ny) in neighbours {
                if nx < SIDE && ny < SIDE {
                    let ni = ny * SIDE + nx;
                    if mask[ni] && labels[ni] == 0 {
                        labels[ni] = label;
                        stack.push(ni);
                    }
                }
            }
        }

        if area < DET_MIN_AREA {
            continue;
        }
        let mean = score / area as f32;
        if mean < DET_BOX_SCORE {
            continue;
        }

        let w = (max_x - min_x + 1) as f32;
        let h = (max_y - min_y + 1) as f32;
        if w < 3.0 || h < 3.0 {
            continue;
        }
        let delta = ((w * h * DET_UNCLIP_RATIO) / (2.0 * (w + h))).ceil() as i32;
        boxes.push((
            min_x - delta,
            min_y - delta,
            max_x + delta,
            max_y + delta,
            area,
            mean,
        ));
    }

    boxes.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    boxes
        .into_iter()
        .map(|(x0, y0, x1, y1, _, _)| DecodedBox {
            x0: x0.clamp(0, SIDE as i32 - 1),
            y0: y0.clamp(0, SIDE as i32 - 1),
            x1: x1.clamp(0, SIDE as i32 - 1),
            y1: y1.clamp(0, SIDE as i32 - 1),
        })
        .collect()
}

/// Extractor for standalone images. Registered only while the OCR weights are
/// present, so a keyword-only or model-less install does not claim to support
/// images it cannot read.
pub struct ImageOcr;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tif", "tiff", "webp", "bmp", "gif"];

impl Extractor for ImageOcr {
    fn name(&self) -> &'static str {
        "image-ocr"
    }

    fn extensions(&self) -> &'static [&'static str] {
        if models_present() {
            IMAGE_EXTENSIONS
        } else {
            &[]
        }
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        models_present() && looks_like_image(bytes)
    }

    fn capabilities(&self) -> super::Capabilities {
        super::Capabilities {
            max_file_bytes: 25 * 1024 * 1024,
            max_pages: 1,
            max_entries: 1,
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        if !models_present() {
            return Ok(None);
        }
        let text = match recognize_bytes(bytes) {
            Ok(text) => text,
            Err(err) => {
                return Ok(Some(single(
                    name,
                    "image-ocr",
                    Vec::new(),
                    vec![format!("OCR failed: {err:#}")],
                )))
            }
        };
        if text.trim().is_empty() {
            return Ok(Some(single(
                name,
                "image-ocr",
                Vec::new(),
                vec!["no text recognised in image".to_string()],
            )));
        }
        let line_end = text.lines().count().max(1) as i64;
        Ok(Some(single(
            name,
            "image-ocr",
            vec![Block {
                text,
                page: 1,
                anchor: Some("image (OCR)".to_string()),
                section: Some("OCR".to_string()),
                line_start: 1,
                line_end,
            }],
            Vec::new(),
        )))
    }
}

fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"II*\x00")
        || bytes.starts_with(b"MM\x00*")
        || (bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"BM")
}

/// Model files, with the SHA-256 of the exact bytes we ship against.
///
/// Pinning the hash means a compromised or mutated upstream mirror cannot feed
/// the OCR engine different weights than the ones this release was tested with.
const MODEL_FILES: [(&str, &str, u64, &str); 3] = [
    (
        DET_FILE,
        "d2a7720d45a54257208b1e13e36a8479894cb74155a5efe29462512d42f49da9",
        4_745_517,
        "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv4/ch_PP-OCRv4_det_infer.onnx",
    ),
    (
        REC_FILE,
        "48fc40f24f6d2a207a2b1091d3437eb3cc3eb6b676dc3ef9c37384005483683b",
        10_857_958,
        "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv4/ch_PP-OCRv4_rec_infer.onnx",
    ),
    (
        DICT_FILE,
        "a1c84d9bdb9ab29043c58896224d32941783eb821629618416dcb08f12886492",
        26_250,
        "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/main/ppocr/utils/ppocr_keys_v1.txt",
    ),
];

/// Download any missing OCR weights into [`model_dir`].
///
/// Called from the first-run bootstrap, so OCR is available without the user
/// knowing a model exists — the same contract as the embedding models. Offline
/// runs return an error and OCR silently stays disabled for that run.
pub async fn ensure_models() -> Result<()> {
    if models_present() {
        return Ok(());
    }
    if offline() {
        bail!("OCR models are missing and PAPER_OFFLINE is set");
    }
    let dir = model_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("paper/", env!("CARGO_PKG_VERSION")))
        .build()?;
    for (name, expected, size, url) in MODEL_FILES {
        let dest = dir.join(name);
        if dest.is_file() && sha256_of(&dest).ok().as_deref() == Some(expected) {
            continue;
        }
        tracing::info!("downloading OCR model {name} ({size} bytes)");
        let bytes = client
            .get(url)
            .send()
            .await
            .with_context(|| format!("download {url}"))?
            .error_for_status()
            .with_context(|| format!("download {url}"))?
            .bytes()
            .await
            .with_context(|| format!("read {url}"))?;
        if bytes.len() as u64 != size {
            bail!(
                "{name}: expected {size} bytes from {url}, received {}",
                bytes.len()
            );
        }
        let digest = hex::encode(Sha256::digest(&bytes));
        if digest != expected {
            bail!("{name}: checksum mismatch (expected {expected}, got {digest})");
        }
        // Write via a temporary file so an interrupted download can never be
        // mistaken for a complete model by the next run.
        let tmp = dest.with_extension("part");
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &dest).await?;
    }
    Ok(())
}

fn offline() -> bool {
    std::env::var_os("PAPER_OFFLINE")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false)
}

fn sha256_of(path: &std::path::Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn image_magic_is_recognised() {
        assert!(looks_like_image(b"\x89PNG\r\n\x1a\nrest"));
        assert!(looks_like_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!looks_like_image(b"%PDF-1.7"));
    }

    #[test]
    fn extensions_hidden_without_models() {
        // The test environment has no weights, so the extractor must not
        // advertise image support.
        if !models_present() {
            assert!(ImageOcr.extensions().is_empty());
        }
    }
}
