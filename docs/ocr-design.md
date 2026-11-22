# OCR design

How Paper reads scanned PDFs and images, on macOS, Linux and Windows, with no
system dependencies and no user configuration.

## Problem

Roughly a third of real-world PDFs in a Downloads folder are scans: the page is
one bitmap with no text layer. Before this change Paper indexed the text layer
only, so a scanned contract was silently absent from search results — the worst
possible failure for a tool whose promise is "every claim carries its source".

Standalone images (`.png`, `.jpg`, photos of whiteboards, receipts) were not
indexed at all.

## Constraints

- **No C/C++ dependency.** The project ships a static musl binary and dropped
  every C++ dependency in 0.2.5. Tesseract (`leptess`, `tesseract-sys`), ONNX
  Runtime (`ort`), pdfium and poppler are therefore out.
- **No system binaries.** `pdftoppm` and `tesseract` cannot be assumed on a
  user's machine, and a missing binary must never fail an index run.
- **Permissive licences only.** No copyleft, no unclear licence.
- **Zero-touch.** A user who installs Paper must get OCR without discovering a
  flag or installing anything else.

## Decision

| Piece | Choice | Why |
|---|---|---|
| Inference | `tract-onnx` (pure Rust, Apache-2.0/MIT) | No C/C++, builds on musl, small |
| Models | PaddleOCR **PP-OCRv4** det + rec ONNX (Apache-2.0) | Accurate, 6.6k-character Latin+CJK dictionary, ~15.6 MB |
| Dictionary | `ppocr_keys_v1.txt` (Apache-2.0) | Required by the recogniser's CTC head |
| Image decode | `image` 0.25 (MIT/Apache-2.0) | Pure Rust, covers JPEG/PNG/TIFF/WebP/BMP/GIF |
| PDF page images | `lopdf` `get_page_images` + `decompressed_content` | Most scans carry one embedded full-page JPEG, so no rasteriser is needed |

### Rejected options

- **`ocrs`** — pure Rust and initially chosen, but its weights are trained on
  HierText and inherit **CC BY-SA 4.0**. Copyleft, so Paper cannot download or
  redistribute them.
- **`rapidocr-core` / `ppocr-rs`** — wrap `ort` (ONNX Runtime, C++).
- **Tesseract** — C++ toolchain plus a system library.
- **macOS Vision / Windows Media OCR** — excellent accuracy with zero download,
  but two extra platform code paths and no Linux equivalent. Kept as future
  optional fast paths; the shipped engine is identical on all three systems.

## Pipeline

### Images

1. Extensions `png, jpg, jpeg, tif, tiff, webp, bmp, gif` (first frame) are
   registered **only when the OCR weights exist**, so a machine without them
   reports the file as unsupported instead of indexing nothing.
2. Decode, clamp the long edge to 2000 px, and lift images smaller than 1600 px
   (72 dpi PDF page images lose word spacing at their native size).
3. Detect at 960×960, crop each line, recognise at height 48 with
   aspect-preserving padding.
4. Emit one block: anchor `image (OCR)`, section `OCR`, joined by newlines.

### PDFs

1. `lopdf` text extraction per page, as before.
2. A page with fewer than 32 non-whitespace characters counts as scanned.
3. For those pages, take the page's **largest embedded image** (a letterhead
   logo is small, the scan is not):
   - `DCTDecode` → the stream content is already JPEG;
   - `FlateDecode` / no filter → `decompressed_content()` with 8-bit
     `DeviceRGB` or `DeviceGray` pixels.
4. OCR the image and append a block with anchor `page N (OCR)`, section `OCR`.
   Any stray real text on the page is kept alongside it.
5. A page with no decodable image (vector-only, CCITT, JBIG2, JPX) produces the
   warning `page N: no text layer and no decodable image` — never a silent skip.

### Robustness

- OCR is best-effort: a missing model, unreadable page or decode failure
  downgrades to "no text" and never aborts an index run.
- The per-extractor timeout rises from 5 s to 180 s when OCR is enabled, because
  a scanned document legitimately needs minutes.
- The engine is a process-wide `OnceLock`, so the models are loaded once per run.

### Model download (zero-touch)

`bootstrap::ensure_models` downloads the three files to
`$PAPER_HOME/models/paddleocr/` on first run, exactly like the embedding and
reranker models. Every file is size- and SHA-256-verified before use, written to
a `.part` file first so an interrupted download can never be mistaken for a
complete model. `PAPER_OFFLINE=1` disables the download.

## Configuration

| Knob | Effect |
|---|---|
| default | OCR on once the weights exist |
| `--ocr` | force the attempt (visible failure if models are missing) |
| `--no-ocr` | skip OCR for this run |
| `PAPER_NO_OCR=1` | disable OCR for every run |
| `paper doctor` | prints engine, model presence and active state |

## Tests

- Unit: image magic-byte sniffing, extension hiding without weights.
- Manual/CI smoke: a rendered PNG and a scanned PDF are indexed and their text
  found by `paper search` (verified locally: reference image text round-trips
  exactly).
- CI keeps building all features on Linux, macOS and Windows; the OCR path adds
  no native toolchain requirement.

## Licences / NOTICE

`tract-onnx`, `image` and their trees are MIT/Apache-2.0 and appear in `NOTICE`
via `scripts/generate-notice.sh`. Model weights are not bundled in the binary;
`NOTICE` records them in a separate section with their upstream sources and
Apache-2.0 licence.
