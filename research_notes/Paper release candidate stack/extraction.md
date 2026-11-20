# Document/Text Extraction Libraries for Paper

## PDF — pure-Rust vs native-binding extractors

### Takeaway
For a local-first MIT/Apache-2.0 single binary that must guarantee exact page citations, the best pure-Rust choice is **lopdf** (page-scoped text, full object model, MIT/Apache-2.0, actively maintained) paired with **pdfluent-extract** (positioned text blocks/bboxes per page, MIT/Apache-2.0) when bbox fidelity is needed. **pdf-extract** (MIT, pure Rust) is a simpler per-page fallback. **pdfium-render** (MIT/Apache-2.0, native binding to Google PDFium) gives the highest-fidelity extraction + page-to-image rendering for an OCR fallback, but requires shipping/bundling the PDFium shared library at runtime — a real deployment cost for a ~10 MB single binary. **hayro** (Apache-2.0/MIT, pure Rust) is a page rasterizer, not a text extractor; useful only as an image source for OCR. **mupdf-rs** is **AGPL-3.0** and therefore excluded by the permissive-license constraint (it also requires a C/C++ toolchain + libclang at build time).

### Cited Findings
- **pdf-extract 0.12.1** (MIT, pure Rust): `extract_text_by_pages(path) -> Vec<String>` returns text per page in document order; `extract_text` returns a single string. Releases ~45 versions, latest 2 months ago, ~8.8K SLoC, 95.7 KiB. No native deps. Owner: Jeff Muizelaar (jrmuizelaar). [crates.io/crates/pdf-extract](https://crates.io/crates/pdf-extract); [docs.rs/pdf-extract/latest/pdf_extract/fn.extract_text_by_pages.html](https://docs.rs/pdf-extract/latest/pdf_extract/fn.extract_text_by_pages.html)
- **lopdf** (MIT/Apache-2.0, pure Rust): `Document::extract_text(&[page_numbers])` and `extract_text_chunks` extract text from given page numbers; `get_pages()` yields page count. Bounds decompressed page size to bound zip bombs. No bounding-box API out of the box (issue #185 confirms bbox is not provided). Active: CHANGELOG entries through v0.34.0 (2024-08-31) with ToUnicode CMap, AES rev 4, PDFDocEncoding support. [crates.io/crates/lopdf](https://docs.rs/lopdf/latest/lopdf/struct.Document.html); [github.com/J-F-Liu/lopdf/blob/main/CHANGELOG.md](https://github.com/J-F-Liu/lopdf/blob/main/CHANGELOG.md)
- **pdfluent-extract 1.4.0** (MIT/Apache-2.0, pure Rust, builds on lopdf::Document): `extract_page_text(&doc, 1)` returns TextBlock records with page number, font, bbox; `PositionedChar` per-character bboxes; `SearchResult` with page + bboxes. Good for citation anchoring with position metadata. [docs.rs/pdfluent-extract/latest/pdfluent_extract](https://docs.rs/pdfluent-extract/latest/pdfluent_extract)
- **pdfium-render 0.9.5** (MIT OR Apache-2.0, native binding): high-level Rust wrapper around Pdfium (Chrome's PDF engine). Per-page text extraction, word/char bboxes, page-to-image rendering (JPEG/PNG), form fields, annotations. Binds to a Pdfium library at run-time (dynamic linking; system-provided or bundled). MSRV 1.61. 262K SLoC, 3.88 MiB crate. Latest release 19 days ago (v0.9.5). Requires `libpdfium` available at runtime on all three OSes (brew install pdfium / apt libpdfium-dev / pdfium.dll). [crates.io/crates/pdfium-render/0.9.5](https://crates.io/crates/pdfium-render/0.9.5); [github.com/ajrcarey/pdfium-render](https://github.com/ajrcarey/pdfium-render)
- **hayro 0.8.0** (Apache-2.0 OR MIT, pure Rust): PDF rasterizer — renders pages to bitmaps/SVG. Read-only, no_std-friendly syntax crate (hayro-syntax). MSRV 1.92. `embed-fonts` feature bundles the 14 base14 fonts. 1,600+ visual-regression PDFs scraped from pdf.js/PDFBox test suites = high rendering correctness. Does NOT do text extraction natively; you'd write a custom `Device` over `hayro-interpret` to extract text. No encrypted/PDF password support. Latest v0.8.0 released 21 days ago. [crates.io/crates/hayro/0.8.0](https://crates.io/crates/hayro/0.8.0); [github.com/LaurenzV/hayro](https://github.com/laurenzv/hayro)
- **mupdf-rs / mupdf-sys 0.8.0** (AGPL-3.0): Safe Rust bindings to MuPDF — text extraction with words+bboxes, page rendering, annotations, encryption. **AGPL-3.0 is a hard exclusion** under Paper's permissive-license rule (MIT/Apache-2.0/BSD only). Requires C/C++ toolchain + libclang for bindgen; builds MuPDF from source (~8-14 MB crate, 138K SLoC). Latest v0.8.0 released ~4 months ago. [crates.io/crates/mupdf/0.8.0](https://crates.io/crates/mupdf/0.8.0); [github.com/messense/mupdf-rs](https://github.com/messense/mupdf-rs)
- Cross-cutting notes: pure-Rust PDF text extraction depends on `/ToUnicode` CMaps for reliable Unicode/CID mapping; lopdf added ToUnicode CMap support in v0.34.0. Reading order is *not* guaranteed by any pure-Rust crate — PDF stores positioned glyphs, not paragraphs; multi-column/interleaved text is a known weakness (pdfium-render is "noticeably better" here per WebScraping.AI comparison). Scanned/image-only PDFs have no text layer; all extractors return empty strings for those pages — OCR fallback is required. [webscraping.ai blog](https://webscraping.ai/blog/web-scraping-with-rust); [PDF Oxide text extraction](https://pdf.oxide.fyi/rust/docs/extraction/text)

### Inferences
- Paper should adopt lopdf as the default PDF text extractor (pure Rust, page-scoped = exact `page` citation anchor, no native deps, small binary impact) with pdfluent-extract layered on top when positioned/bbox text is needed.
- pdfium-render should be an optional, gated native-dependency accelerator (feature-flagged), not a default dependency, because it breaks the single-binary ~10 MB goal and requires runtime PDFium on all three OSes.
- hayro is a candidate OCR-fallback image source (render page -> bitmap -> OCR engine) but is not itself a text extractor and lacks encrypted-PDF support.
- mupdf-rs is explicitly ruled out (AGPL-3.0 + native toolchain).

### Gaps
- No measured binary-size delta for adding lopdf vs pdf-extract vs pdfluent-extract to Paper's release binary.
- No per-library benchmark on malformed/encrypted/truncated PDFs beyond lopdf's decompress-size guard and hayro's regression-suite claim.
- Unicode/CID reliability for specific CJK/Identity-H fonts without `/ToUnicode` not quantified for any pure-Rust crate.


## Office formats — DOCX, XLSX, PPTX, legacy DOC/XLS/PPT, ODT/ODS/ODP, RTF

### Takeaway
The Office ecosystem is well served by maintained pure-Rust crates under permissive licenses: **docx-rs** (MIT, active through Jul 2026) for DOCX, **calamine** (MIT, active) for XLSX/XLS/ODS/XLSB with sheet+cell addressing, and **pptxboss-core / ppt-rs / rustypptx** (MIT/Apache-2.0) for PPTX with per-slide text and slide numbers. Legacy binary DOC/XLS/PPT is the weak spot: calamine covers XLS/XLSB but there is no maintained pure-Rust DOC or PPT reader — legacy formats require LibreOffice shell-out or are unsupported. RTF has several small pure-Rust parsers but none clearly production-grade; **rtf-parser 0.4.3** (MIT, Jun 2026) is the most recently maintained but panics on lone surrogates and cannot ingest code-page RTF bytes; **rtf-parser-tt 0.6.0** adds code-page support. ODT/ODP (OpenDocument text/presentation) have no dedicated maintained Rust reader — calamine covers ODS (spreadsheet) only.

### Cited Findings
- **docx-rs 0.4.22** (MIT, pure Rust): reads and writes .docx via `read_docx(&bytes) -> Docx`. Exposes paragraphs, runs, tables (incl. nested), headings, styles, headers/footers, footnotes, images, TOC, sections. Changelog active: v0.4.22 (21 Jul 2026), v0.4.20 (2 Apr 2026), v0.4.19 (8 Feb 2026). 41 releases, used in 130 crates. No native deps. Anchor: paragraph/run order within the document body — no native page concept (DOCX is flow layout; page numbers require rendering). [lib.rs/docx-rs](https://lib.rs/docx-rs); [github.com/bokuweb/docx-rs/blob/main/CHANGELOG.md](https://github.com/bokuweb/docx-rs/blob/main/CHANGELOG.md); [docs.rs/docx-rs/latest/docx_rs](https://docs.rs/docx-rs/latest/docx_rs)
- Note on docx-rs suitability: Tritium (legal tech) outgrew docx_rs because it is "designed for construction, not consumption" — deeply nested tables/paragraphs are hard to model in Rust. For Paper's read-only extraction this is a caution, not a blocker. [tritium.legal/blog/word](https://tritium.legal/blog/word)
- **calamine** (MIT, pure Rust): reads xls, xlsx, xlsm, xlsb, xla, xlam, ods. `open_workbook_auto` infers type. `worksheet_range("Sheet1") -> Range<Data>` gives row/col cell addressing; `sheet_names()`, `worksheet_cells_reader` for per-cell iteration, `Hyperlink`, `Table` (named Excel tables). Cell positions are absolute `(row, column)` worksheet coordinates — direct citation anchor as `sheet!row:col`. Read-only (no write). Active repo (1,090 commits, created 2016). `picture` feature for embedded images. [github.com/tafia/calamine](https://github.com/tafia/calamine); [docs.rs/calamine/latest/calamine/struct.Xlsx.html](https://docs.rs/calamine/latest/calamine/struct.Xlsx.html); [crates.io/crates/calamine](https://crates.io/crates/calamine)
- **pptxboss-core 0.3.0 / ppt-rs 0.2.37 / rustypptx / pptx 0.1.0** (MIT or MIT/Apache-2.0, pure Rust): read .pptx; pptxboss-core also reads legacy .ppt (MS-CFB/MS-PPT/MS-ODRAW) into the same slide model. `Document::open`, `slide.number()`, `slide.text()`, `slide.notes_text()`. Slides are addressable by number — direct citation anchor as `slide N`. pptxboss is lenient (tolerates broken archives), parallel slide parsing, no C dependencies. [crates.io/crates/pptxboss-core/0.3.0](https://crates.io/crates/pptxboss-core/0.3.0); [crates.io/crates/ppt-rs/0.2.37](https://crates.io/crates/ppt-rs/0.2.37); [github.com/longway-code/rustypptx](https://github.com/longway-code/rustypptx); [crates.io/crates/pptx](https://crates.io/crates/pptx)
- **RTF**: `rtf-parser 0.4.3` (MIT, Jun 2026 — most maintained, decodes `\uN`, but panics on lone surrogates, takes &str so cannot ingest real code-page RTF bytes, pulls 25 crates incl. two copies of syn + wasm-bindgen — rejected per rclip-rtf review). `rtf-parser-tt 0.6.0` (fork of rtf-parser, handles code pages / any declared code page, `from_bytes`). `scrivener-rtf 0.1.0` (MIT, Mar 2026, 4 commits, AST parser, no_std-friendly). None have significant reverse dependencies or production track record. [crates.io/crates/rtf-parser](https://docs.rs/crate/rtf-parser/latest); [crates.io/crates/rtf-parser-tt](https://crates.io/crates/rtf-parser-tt); [crates.io/crates/scrivener-rtf](https://crates.io/crates/scrivener-rtf); [rclip-rtf prior-art review](https://crates.io/crates/rclip-rtf)
- **EPUB**: `rbook 0.7.12` (Apache-2.0, pure Rust) — EPUB 2/3, OPF metadata, manifest, spine reading order, ToC, streaming reader; used by omniparse for EPUB. `epub-parser 0.3.4` alternative (zip + quick-xml). Anchor: spine document index + optional fragment. [docs.rs/rbook](https://docs.rs/rbook); [github.com/DevinSterling/rbook](https://github.com/DevinSterling/rbook)
- **HTML boilerplate removal**: `libreadability` (Rust port of readeck/readability Go port of Mozilla Readability.js — Article with content + text_content + metadata). `dom_smoothie 0.19.0` (MIT, readability.js port). `legible` (Apache-2.0, semantic candidates, Markdown/HTML/text output). `rs-trafilatura` (Apache-2.0, ML page-type classification, F1=0.966 ScrapingHub). `justext-rs` (BSD-2-Clause, paragraph-level). `decruft 0.2` (MIT, site-specific extractors + ~500 class/id patterns). All pure Rust, no system libs, no network. [docs.rs/libreadability](https://docs.rs/libreadability); [crates.io/crates/dom_smoothie](https://crates.io/crates/dom_smoothie); [github.com/ryanfowler/legible](https://github.com/ryanfowler/legible); [github.com/Murrough-Foley/rs-trafilatura](https://github.com/Murrough-Foley/rs-trafilatura); [crates.io/crates/justext](https://crates.io/crates/justext); [crates.io/crates/decruft](https://crates.io/crates/decruft)
- **Email**: `mailparse 0.18.0` (0BSD, pure Rust, MSRV 1.85, ~16M downloads all time) — `parse_mail(&[u8]) -> ParsedMail`, MIME multipart, headers, body subparts. `mail-parser` (stalwartlabs, zero-copy, fuzzed, used in Stalwart Mail Server at scale) with `mbox::MessageIterator`. `msg_parser 0.3.6` (MIT) — Outlook .msg (OLE compound doc) metadata/body/recipients/attachments. Anchor: message headers + body part; mbox message index within file. [crates.io/crates/mailparse/0.18.0](https://crates.io/crates/mailparse/0.18.0); [crates.io/crates/mail-parser](https://crates.io/crates/mail-parser); [crates.io/crates/msg_parser/0.3.6](https://crates.io/crates/msg_parser/0.3.6)
- **Archives**: ZIP/TAR via standard `zip` and `tar` crates (pure Rust, MIT/Apache-2.0). omniparse lists ZIP/TAR with path-traversal detection as supported. No native deps. [docs.rs/omniparse supported formats](https://docs.rs/omniparse/latest/omniparse/)

### Inferences
- Paper's Office layer: docx-rs (DOCX) + calamine (XLSX/XLS/ODS) + pptxboss-core/ppt-rs (PPTX) covers all modern Office formats with pure-Rust, permissive-licensed, actively maintained crates, each exposing a natural citation anchor (paragraph order, sheet!row:col, slide N).
- Legacy DOC and PPT (OLE compound binary) are not covered by any maintained pure-Rust crate — mark as "unsupported without LibreOffice shell-out" or gate behind an optional feature.
- ODT (text) and ODP (presentation) are not covered — calamine only does ODS. Mark ODT/ODP as unsupported or shell-out.
- RTF: no clearly production-grade parser; rtf-parser-tt (code pages) is the most feature-complete but unproven. Consider RTF low priority or shell-out to textutil on macOS.
- Markdown with frontmatter: Paper already indexes UTF-8 text; frontmatter (YAML/TOML) can be stripped by the existing text pipeline or a small serde-based stripper — no new crate needed.

### Gaps
- No maintained pure-Rust reader for legacy .DOC or .PPT (OLE2) — confirm whether any crate exists beyond calamine's XLS/XLSB.
- No maintained pure-Rust reader for ODT (text) or ODP (presentation).
- RTF parser production maturity unverified beyond the rclip-rtf critique of rtf-parser.
- Frontmatter stripping approach for Markdown not yet specified in Paper's codebase.


## OCR for scanned receipts/photos and image-only PDFs

### Takeaway
Pure-Rust OCR options are immature: **ocrs 0.14.0** (Apache-2.0/MIT, pure Rust, ONNX models via rten) is the most downloaded pure-Rust engine (~347K downloads) but is "early preview", Latin-alphabet only, English-only in practice, and downloads models on first run (violates Paper's fully-local rule unless models are pre-bundled). **Tesseract via bindings** (`tesseract-rs 0.4.0` MIT; **kreuzberg-tesseract 1.0.0-rc.1**, a maintained fork) compiles Tesseract 5.5 + Leptonica from C++ source at build time, embeds tessdata, supports 100+ languages, and gives word-level bounding boxes — but it is a native C++ dependency (~several MB, requires C++17 toolchain), breaking the pure-Rust single-binary ideal. The pragmatic cross-OS answer is **OS-native OCR**: **apple-vision 0.17.1** (MIT/Apache-2.0, macOS 13+, Xcode SDK) on macOS; Windows.Media.OCR on Windows (no mature Rust crate found — gap); Tesseract CLI/library on Linux. A multi-provider abstraction (as in **uni-ocr 0.1.5**, MIT/Apache-2.0) lets Paper prefer native OCR on macOS/Windows and fall back to Tesseract on Linux, all fully local and account-free.

### Cited Findings
- **ocrs 0.14.0** (Apache-2.0 OR MIT, pure Rust): neural OCR engine, detection + recognition models exported from PyTorch to ONNX, run via rten tensor engine. Latin alphabet only (English), calls itself "early preview", expects more errors than commercial engines. Models downloaded to ~/.cache/ocrs on first run (not fully local unless pre-bundled). Works in WebAssembly. `OcrEngine::new`, `detect_words`, `find_text_lines`, `recognize_text`, `get_text`. [github.com/robertknight/ocrs](https://github.com/robertknight/ocrs); [crates.io/crates/ocrs](https://crates.io/crates/ocrs); [docs.rs/ocrs/latest/ocrs](https://docs.rs/ocrs/latest/ocrs)
- **tesseract-rs 0.4.0** (MIT): Rust bindings for Tesseract OCR with built-in compilation of Tesseract + Leptonica. MSRV 1.88. 89 KiB crate but pulls in C++ toolchain at build. Embeds tessdata (English + Turkish by default; `TESSERACT_EMBED_LANGUAGES` to select). Supports Linux/macOS/Windows/FreeBSD. `TesseractAPI::init(tessdata_dir, lang)`. [crates.io/crates/tesseract-rs/0.4.0](https://crates.io/crates/tesseract-rs/0.4.0)
- **kreuzberg-tesseract 1.0.0-rc.1**: maintained fork of tesseract-rs with C++17 support (Tesseract 5.5.1 requires it), cross-compilation fixes, Windows MSVC static linking, build caching. Static linking default (no runtime deps) or dynamic linking to system libtesseract/libleptonica. macOS: `brew install tesseract leptonica`. [crates.io/crates/kreuzberg-tesseract](https://crates.io/crates/kreuzberg-tesseract)
- **tesseract5-rs 0.1.4**: thin ergonomic layer over tesseract-rs (dariofinardi fork, Tesseract 5.5.0 + Leptonica 1.85), word-level bounding-box hierarchy, `OcrOptions { lang, psm, tessdata_dir, with_hierarchy }`. MIT. [crates.io/crates/tesseract5-rs/0.1.4](https://crates.io/crates/tesseract5-rs/0.1.4)
- **apple-vision 0.17.1** (MIT OR Apache-2.0): safe Rust bindings to Apple Vision framework — on-device OCR (`VNRecognizeTextRequest`), macOS 13+, requires Xcode + macOS SDK (build compiles a Swift bridge). `TextRecognizer` with `RecognitionLevel::Accurate`, language correction. ~12K SLoC, 129 KiB. Swift-only macOS 26 APIs (RecognizeDocumentsRequest) not wrapped. Native, on-device, no network, no account. [crates.io/crates/apple-vision/0.17.1](https://crates.io/crates/apple-vision/0.17.1); [github.com/doom-fish/vision-rs](https://github.com/doom-fish/vision-rs)
- **objc2-vision 0.3.2** (Zlib OR Apache-2.0 OR MIT): lower-level Objective-C bindings to Vision framework, part of the objc2 project. Alternative to apple-vision if a lighter FFI surface is wanted. [crates.io/crates/objc2-vision/0.3.2](https://crates.io/crates/objc2-vision/0.3.2)
- **uni-ocr / uni-ocr 0.1.5** (MIT/Apache-2.0): unified OCR engine — providers: macOS VisionKit, Windows OCR engine, Tesseract, cloud providers (gated, not for Paper). `OcrEngine::new(OcrProvider::Auto)` picks best available. Benchmarks on M4 Max: macOS Vision 3.2 img/s @90% accuracy; Windows OCR 1.2 img/s @95.2%. Repo activity is light (39 commits, created Mar 2025). [github.com/screenpipe/uniOCR](https://github.com/screenpipe/uniOCR); [crates.io/crates/uni-ocr/0.1.5](https://crates.io/crates/uni-ocr/0.1.5)
- **omniparse OCR** (Apache-2.0): feature-gated OCR gate — `OMNIPARSE_OCR=classical` (pure-Rust classical pipeline) or `=ml` (ocrs + rten) or off. Image/PDF parsers route through OCR when gated, populate `ocr_status/ocr_confidence/ocr_applied` metadata. Model-cache CLI pre-fetches + SHA-256-verifies models. [docs.rs/omniparse/latest/omniparse](https://docs.rs/omniparse/latest/omniparse)
- Accuracy/language coverage (from uniocr benchmarks + general knowledge): macOS Vision ~90%; Windows OCR ~95%; Tesseract engine quality varies by language, best on Latin scripts with clean 300dpi scans, poor on receipts/photos at low resolution. ocrs: Latin/English only. No Rust-native engine matches Tesseract's 100+ language coverage.

### Inferences
- Paper should treat OCR as an optional, gated feature (off by default, gated behind `--ocr` or a feature flag), consistent with invariant #1 (search never downloads a model — missing OCR weights means image-only PDFs yield empty text plus a hint).
- For scanned PDFs/image fallback: render page to bitmap (pdfium-render or hayro) then OCR. Prefer OS-native OCR (Apple Vision on macOS, Windows.Media.OCR on Windows, Tesseract on Linux) to avoid shipping C++ ML runtimes; abstract behind a small trait so the rest of Paper stays pure Rust.
- Tesseract bindings (kreuzberg-tesseract) are the fallback for Linux and for any language beyond what native OS OCR covers, but require a C++17 toolchain at build time and tessdata files — quantify binary size impact before enabling.
- ocrs is not suitable as Paper's default OCR: English-only, downloads models, early-preview quality.

### Gaps
- No mature Rust crate for Windows.Media.OCR found — confirm whether one exists or whether Windows OCR requires a separate interop layer.
- Binary-size impact of static Tesseract + Leptonica + embedded tessdata on Paper's release binary not measured.
- OCR accuracy on receipts/low-res photos not measured for any engine in this scope.


## Recommended library summary (per format)

| Format | Recommended crate | Version | License | Maintenance | Native deps? | mac/Win/Linux | Citation anchor |
|--------|------------------|---------|---------|-------------|--------------|---------------|-----------------|
| PDF (text layer) | lopdf + pdfluent-extract | lopdf latest; pdfluent-extract 1.4.0 | MIT/Apache-2.0 | Active | None (pure Rust) | All three | Page number (+ bbox via pdfluent-extract) |
| PDF (fallback/simple) | pdf-extract | 0.12.1 | MIT | Active (45 versions, latest 2mo) | None | All three | Page number |
| PDF (native, high fidelity) | pdfium-render | 0.9.5 | MIT/Apache-2.0 | Active (19d ago) | libpdfium runtime | All three (lib must be present) | Page number (+ bboxes) |
| PDF (rasterize for OCR) | hayro | 0.8.0 | Apache-2.0/MIT | Active (21d ago) | None | All three | Page number (as image source) |
| PDF (excluded) | mupdf-rs | 0.8.0 | AGPL-3.0 | Active | C toolchain + libclang | All three | — |
| DOCX | docx-rs | 0.4.22 | MIT | Active (Jul 2026) | None | All three | Paragraph/run order (no native page) |
| XLSX / XLS / ODS / XLSB | calamine | latest | MIT | Active | None | All three | Sheet + row + column (cell ref) |
| PPTX (+ legacy .ppt) | pptxboss-core / ppt-rs | 0.3.0 / 0.2.37 | MIT / MIT+Apache-2.0 | Active | None | All three | Slide number |
| Legacy .DOC / .PPT | none maintained | — | — | — | — | — | — |
| ODT / ODP | none maintained | — | — | — | — | — | — |
| RTF | rtf-parser-tt | 0.6.0 | MIT | Low (fork) | None | All three | Document order |
| EPUB | rbook | 0.7.12 | Apache-2.0 | Active | None | All three | Spine doc index + fragment |
| HTML (boilerplate removal) | libreadability / dom_smoothie / legible | various | Apache-2.0 / MIT / Apache-2.0 | Active | None | All three | N/A (no page/line in raw HTML) |
| Email (eml/mime) | mailparse / mail-parser | 0.18.0 / stalwart | 0BSD / (stalwart) | Active | None | All three | Message headers + body part |
| Email (mbox) | mail-parser mbox::MessageIterator | stalwart | — | Active | None | All three | Message index within mbox |
| Email (.msg Outlook) | msg_parser | 0.3.6 | MIT | Low (59 commits, 2021) | None | All three | Message headers + body |
| Markdown (frontmatter) | (strip in-text pipeline; serde YAML/TOML optional) | — | — | — | None | All three | Byte offset / line |
| Images (OCR source) | hayro render → OS-native OCR | — | — | — | None (render) | All three | Page number |
| OCR (macOS) | apple-vision | 0.17.1 | MIT/Apache-2.0 | Active (3h ago at fetch) | Xcode SDK (build only) | macOS only | — |
| OCR (Windows) | Windows.Media.OCR | — | — | — | — | Windows only | — |
| OCR (Linux / multi-lang) | kreuzberg-tesseract | 1.0.0-rc.1 | — | Maintained fork | C++17 toolchain, tessdata | Linux/Win/Mac | — |
| OCR (pure Rust) | ocrs | 0.14.0 | Apache-2.0/MIT | Early preview | None | All three | — |
| Archives (zip/tar) | zip / tar crates | — | MIT/Apache-2.0 | Active | None | All three | Entry path within archive |

### Inferences
- The default stack is entirely pure-Rust, permissive-licensed, actively maintained, and builds on all three OSes with no native dependencies — consistent with Paper's ~10 MB single-binary and local-first constraints.
- The only native-dependency candidates are pdfium-render (runtime libpdfium) and Tesseract bindings (build-time C++ toolchain + tessdata); both should be feature-gated and off by default.
- Legacy .DOC/.PPT and ODT/ODP have no maintained pure-Rust reader — explicitly document as unsupported or shell-out-only.
- Windows OCR lacks a mature Rust crate — the cross-OS OCR abstraction will need a Windows-specific interop path or Tesseract fallback.

### Gaps
- Maintenance "last commit" dates not verified for every crate in the table beyond what crates.io/docs.rs reported.
- Binary sizes for each crate's contribution to a release build not measured.


## Canonical anchoring scheme for formats with no lines

### Takeaway
Paper's core invariant is that every hit carries `path`, `line_start`, `line_end` and chunking keeps them accurate. Formats without lines (PDF page text, spreadsheet cells, slides) need a canonical anchor scheme so citations are stable and resolvable. Recommendation: anchors are format-specific tuples stored alongside the hit, with line-based anchors used where lines exist and a documented fallback elsewhere. Chunking must emit one anchor per chunk and never synthesize line numbers from position.

### Cited Findings
- Paper invariant #2 (Citations must resolve): "Every hit carries `path`, `line_start`, `line_end`; chunking is responsible for keeping them accurate." [AGENTS.md — Invariants]
- Paper currently indexes ONLY UTF-8 text formats with `page=0` always. [AGENTS.md — Layout / Project Context]
- lopdf: `extract_text(&[page_numbers])` gives text per page; page number is the natural anchor. [docs.rs/lopdf](https://docs.rs/lopdf/latest/lopdf/struct.Document.html)
- pdfluent-extract: `TextBlock { page, font, bbox }` and `PositionedChar` give per-character bboxes — page + position anchor. [docs.rs/pdfluent-extract](https://docs.rs/pdfluent-extract/latest/pdfluent_extract)
- calamine: cell positions are absolute `(row, column)` worksheet coordinates; `sheet_names()` gives sheet names — anchor as `SheetName!R1C1` or `SheetName!A1`. [docs.rs/calamine](https://docs.rs/calamine/latest/calamine/struct.Range.html)
- pptxboss-core: `slide.number()` — anchor as slide N. [crates.io/crates/pptxboss-core](https://crates.io/crates/pptxboss-core/0.3.0)
- rbook EPUB: spine gives canonical reading order; each readable resource has a manifest href — anchor as spine index + href + optional fragment. [docs.rs/rbook](https://docs.rs/rbook/latest/rbook/epub)

### Inferences
- **PDF**: anchor = `page N` (1-indexed, matching PDF convention and lopdf/pdfluent-extract). When pdfluent-extract bboxes are available, supplement with bbox but keep page as the citation unit since lines are not well-defined in PDF.
- **Spreadsheets (XLSX/ODS)**: anchor = `SheetName!row:col` (absolute worksheet coordinates from calamine). If a chunk spans multiple cells, record the top-left cell plus extent.
- **Presentations (PPTX)**: anchor = `slide N` (pptxboss-core slide number).
- **DOCX**: no native page concept (flow layout); anchor = paragraph/run ordinal or byte offset within document.xml text. Consider "approx. page" only if a rendering step is added later — out of scope for now.
- **EPUB**: anchor = spine document index + href + optional fragment ID.
- **Email**: anchor = message headers (Message-ID) + body part index; mbox = message index within file.
- **HTML/Markdown/plain text**: anchor = line_start/line_end as today; byte offset as secondary.
- The hit/hydration layer in `src/core/search/` (RRF + confidence + hydration) should carry an optional format-specific anchor field so citations resolve without inventing line numbers.

### Gaps
- Paper's current `hit` struct and chunking pipeline assumed `line_start`/`line_end`; extending to optional format-specific anchors requires code changes not yet specified.
- No decision on whether PDF citations use 0- or 1-indexed pages internally (lopdf uses 1-based page numbers in `extract_text`).


## Shell-out to pdftotext / textutil / LibreOffice as optional accelerators

### Takeaway
Shelling out to system tools is a *supplement*, not a replacement, for Paper's pure-Rust stack. `pdftotext` (Poppler) and `textutil` (macOS) give battle-tested PDF/Office text extraction with good layout/reading-order handling, and LibreOffice headless converts legacy DOC/XLS/PPT/ODT/ODP — but each introduces a hard system dependency, breaks the single-binary guarantee, complicates offline/sandboxed deployments (PAPER_OFFLINE=1), and makes exact page/line citation anchoring harder to control. Recommend: keep pure-Rust crates as the default path; expose shell-out accelerators only behind opt-in feature flags / config, with clear degradation when the binary is absent.

### Cited Findings
- **pdftotext** (Poppler utils): external-command fallback in PDFBench benchmark tool; included by default but optional via feature flag. Requires system Poppler (apt libpoppler-dev / brew install poppler). [github.com/rainrambler/PDFBench](https://github.com/rainrambler/PDFBench)
- **textutil**: macOS-native command that converts between document formats (docx, rtf, html, pdf, etc.); part of macOS, no install needed on Mac, unavailable on Windows/Linux. Good for RTF/DOCX on macOS only.
- **LibreOffice headless**: converts legacy Office (DOC/XLS/PPT) and ODF to text/PDF; large runtime, slow startup, not suitable for a ~10 MB local-first binary; shell-out only. No Rust crate found for legacy DOC/PPT, making LibreOffice the only practical path for those formats.
- Paper invariant #1: "Search never downloads a model. Missing weights mean keyword-only search plus a hint, or exit code 3 when --semantic was requested." — by analogy, missing system binary for shell-out means keyword-only/search-degraded plus a hint, not a hard failure. [AGENTS.md — Invariants]
- Paper invariant: completely local, no telemetry/accounts/cloud — shell-out to local system binaries is acceptable if the binary is present locally, but network-downloading any helper is not.

### Inferences
- Default extraction path: pure-Rust crates only (lopdf, docx-rs, calamine, pptxboss-core, rbook, mailparse, etc.).
- Optional shell-out accelerators (opt-in): `pdftotext` for PDF (better reading order on complex layouts), `textutil` for RTF/DOCX on macOS, `libreoffice` headless for legacy DOC/XLS/PPT/ODT/ODP conversion.
- When the accelerator binary is absent, degrade gracefully to the pure-Rust path with a hint, never a hard error — consistent with Paper's local-first and offline-first design.
- Citation anchoring is harder to guarantee via shell-out (text layout/pagination may differ from Pure-Rust parsing); prefer pure-Rust anchors when both paths are available.

### Gaps
- No benchmark comparing pure-Rust vs pdftotext text quality/citation accuracy on real PDFs.
- No design for how shell-out accelerators are configured (config file? env var? CLI flag?) in Paper.


## Size/complexity impact on the single ~10 MB release binary

### Takeaway
The recommended pure-Rust stack (lopdf + docx-rs + calamine + pptxboss-core + rbook + mailparse + readability/HTML + zip/tar) adds modest compile-time and binary size but stays within a ~10 MB release budget, since all are pure Rust with no native dependencies. The main binary-size risks are the *optional* native-dependency paths: pdfium-render (runtime libpdfium, ~3.88 MiB crate plus the external library) and Tesseract bindings (C++17 build, several MB static, plus tessdata). Keep both strictly feature-gated and off by default so the default release binary remains small, self-contained, and buildable offline.

### Cited Findings
- lopdf: pure Rust, no native deps; crate size modest (docs.rs shows object-model API). [docs.rs/lopdf](https://docs.rs/lopdf/latest/lopdf)
- docx-rs 0.4.22: source code size 3.0 MB (mostly XML schema definitions), but that's source, not compiled binary impact. [crates.io/crates/docx-rs/0.4.22](https://docs.rs/crate/docx-rs/latest)
- pdfium-render 0.9.5: 262K SLoC, 3.88 MiB crate, requires libpdfium at runtime. [crates.io/crates/pdfium-render/0.9.5](https://crates.io/crates/pdfium-render/0.9.5)
- mupdf-sys 0.8.0: 1,227K SLoC, 13.8 MiB crate, AGPL-3.0 — excluded. [crates.io/crates/mupdf-sys/0.8.0](https://crates.io/crates/mupdf-sys/0.8.0)
- tesseract-rs / kreuzberg-tesseract: compile Tesseract 5.5 + Leptonica from C++ source at build time; static linking embeds several MB; tessdata language files add MB per language. [crates.io/crates/kreuzberg-tesseract](https://crates.io/crates/kreuzberg-tesseract)
- ocrs 0.14.0: pure Rust + rten ONNX runtime; models downloaded separately (~models not in binary). [crates.io/crates/ocrs](https://crates.io/crates/ocrs)
- Paper target: single Rust binary ~10 MB release; `cargo build --release` for performance; debug Candle ~15x slower. [AGENTS.md — Build & test]
- `PAPER_OFFLINE=1` forbids network access — any dependency that downloads models/binaries at build or runtime conflicts with this. [AGENTS.md — paths]

### Inferences
- Default build = pure-Rust only, no `build.rs` that invokes a C compiler, no runtime shared libraries → reproducible, offline-buildable, small release binary.
- Feature-gate native paths: `pdfium` feature (pdfium-render), `ocr-tesseract` feature (kreuzberg-tesseract + tessdata), `ocr-native` feature (apple-vision on macOS only). Default features exclude all of these.
- `cargo check` must remain warning-free; adding optional native deps must not break the warning-free guarantee on the default feature set.
- Binary-size measurement: run `cargo build --release` and inspect binary size before/after adding each extraction crate; set a size budget per crate if the release binary approaches the ~10 MB target.

### Gaps
- No measured baseline or per-crate binary size contribution for Paper's release build.
- No size budget allocated per extraction crate.






