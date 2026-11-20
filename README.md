# Paper

[![CI](https://img.shields.io/github/actions/workflow/status/arrowassassin/paperscope/ci.yml?branch=main&label=CI)](https://github.com/arrowassassin/paperscope/actions/workflows/ci.yml)
[![Licence](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue)](#licence)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-black)](#install)

Local-first document search with exact citations. One Rust binary, two front-ends: a CLI and an MCP stdio server. No telemetry, no account, no cloud unless explicitly opted in.

```
$ paper search "when does the fridge warranty expire"
/Users/you/Documents/fridge-warranty.txt:1-4
  Samsung refrigerator warranty certificate ... Coverage ends on 2027-01-15.
  1.000 · matched by keyword search
```

Every hit carries `path` + `line_start` + `line_end`, so a citation always resolves to the exact lines it quotes.

## Install

```bash
cargo install paperscope      # installs the `paper` binary
paper models install bge-small-en-v1.5   # optional: 134 MB, enables semantic search
paper index --root ~/Documents
```

Searching **never** downloads a model. Without weights you get keyword-only search plus a hint; `--semantic` turns a missing model into a hard error (exit code 3) instead of a silent download.

## Commands

```
paper index  --root <dir> [--embeddings] [--file <path>] [--ocr]
paper search <query> [--root <dir>] [--limit N] [--json] [--semantic] [--after DATE] [--before DATE] [--ext pdf,docx] [--path substr]
paper answer <question> [--json]
paper aggregate --field date|amount --op min|max|sum|avg|count [--filter kind=warranty|receipt|invoice|contract]
paper models list|install <id>|remove <id>
paper serve  --mcp [--root <dir>]
paper doctor
```

Exit codes: `0` found, `1` not found, `2` error, `3` model missing.

| Variable | Effect |
| --- | --- |
| `PAPER_HOME` | Overrides the data directory entirely (tests and sandboxes). |
| `PAPER_OFFLINE=1` | Forbids all network access. |

## How it searches

1. **FTS5** keyword search, terms combined with `OR` and ranked by `bm25` (an `AND` of every word makes natural-language questions match nothing).
2. **Early exit** if keyword evidence is already sufficient — no model loads. Skips only when the top hit has full lexical coverage, not merely "≥10 hits".
3. **sqlite-vec KNN** for semantic search when the embedder is present (never downloaded at query time).
4. **RRF(k=60)** fusion: rank-based, avoids score-incompatibility between unbounded BM25 and bounded cosine.
5. **Abstention**: `Confidence::Low` hits are excluded from answers — a plausible-looking quote from a weak match is worse than "not found".

## Formats

| Format | Anchor | Extractor |
|--------|--------|-----------|
| PDF | `page N` | `lopdf` (per page, exact) |
| DOCX | `para N-M` | OOXML XML (paragraph ranges, heading hierarchy) |
| XLSX/XLSM/XLSB/ODS | `Sheet1!A4:H4` | `calamine` (per row, cell ranges) |
| PPTX | `slide N` | OOXML XML (per slide) |
| EPUB | `spine N: chapter.xhtml` | OPF manifest + spine |
| Email | `message <id>` | `mailparse` (headers + body) |
| HTML | heading hierarchy | `quick-xml` (text nodes, source line numbers) |
| Markdown | `#` headings | built-in (exact line ranges) |
| Archives | `outer.zip!/inner/path` | `zip`/`tar` (one document per member) |
| Plain text | line ranges | built-in |

Scanned PDFs return empty pages with a warning. OCR is opt-in via `paper index --ocr` (requires Tesseract + `pdftoppm`).

## Multilingual

CJK (Chinese/Japanese/Korean), Thai, Lao, Khmer, Myanmar are indexed as **overlapping bigrams + first character** — the best-performing dictionary-free scheme measured across all three languages, without shipping a 15–20 MB dictionary. NFKC normalization handles full-width/half-width forms.

Optional dictionary segmenters behind features: `icu-segmenter` (Thai/Lao/Khmer/Myanmar), `lindera` (Japanese), `jieba-rs` (Chinese). Default off to keep the binary small.

## Models

| Model | Size | Default | Purpose |
|-------|------|---------|---------|
| `BAAI/bge-small-en-v1.5` | 134 MB | Yes | English semantic search (384 dims) |
| `intfloat/multilingual-e5-small` | 487 MB | Opt-in | 100 languages (384 dims) |
| `BAAI/bge-reranker-base` | 1.13 GB | Opt-in | Cross-encoder reranking |

All models are Hugging Face Candle (pure Rust, no ONNX/Python), commit-pinned with per-file SHA256.

## MCP Server

```
paper serve --mcp --root ~/Documents
```

Tools: `search_documents`, `read_source`, `list_sources`, `answer_documents`. Root allow-list enforced per request.

## Build & Test

```bash
cargo check --all-targets   # must be warning-free
cargo test                  # 63 tests, offline, no model downloads
cargo build --release       # ~13 MB single binary
cargo deny check licenses bans sources  # no copyleft in shipped tree
cargo bench --bench eval    # retrieval quality CI gate (recall@10 ≥ 0.75, citation accuracy ≥ 0.95)
```

Model integration tests download real weights and are `#[ignore]`d:

```bash
cargo test --test models -- --ignored   # 134 MB / 487 MB / 1.13 GB
```

Always measure performance with `--release`; debug Candle is ~15× slower.

## Packaging

- `cargo-dist` produces tarballs, .deb, .rpm, .apk, Homebrew formula, Scoop manifest, npm package
- macOS notarization, Windows Authenticode, Linux GPG signing in CI
- SBOM (CycloneDX + SPDX) and SLSA Level 3 provenance
- `cargo vet` supply-chain verification (imports from Mozilla, Google, rust-secure-code)

See `docs/PACKAGING.md` for details.

## Licence

MIT OR Apache-2.0. Licence texts in `LICENSE-MIT` and `LICENSE-APACHE`.