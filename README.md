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
paper answer <question> [--json] [--polish [--engine local|openrouter]]
paper aggregate --field date|amount --op min|max|sum|avg|count [--filter kind=warranty|receipt|invoice|contract]
paper models list|install <id>|remove <id>
paper serve  --mcp [--root <dir>]
paper daemon [--foreground] [--root <dir>]...
paper daemon-control status|pause|resume|index|add-root|remove-root|stop|install|uninstall|start|restart
paper doctor
```

Exit codes: `0` found, `1` not found, `2` error, `3` model missing.

## Background indexing (daemon)

`paper daemon` auto-discovers `~/Documents`, `~/Downloads` and `~/Desktop`,
watches for changes (250 ms debounce), reindexes daily at 3 AM plus every
5 minutes incrementally, and pauses on battery <20% or sustained CPU >80%.
Control it over IPC with `paper daemon-control status|pause|resume|stop`, or
install it as a systemd user service (Linux) / launchd agent (macOS):

```bash
paper daemon-control install   # writes + starts the OS service
paper daemon-control status    # indexed files, paused state, model
```

Windows daemon support is not implemented yet.

## Using with agents (MCP)

Any MCP client (Claude Desktop, Claude Code, Codex CLI, OpenCode) can search
your documents through `paper serve --mcp`. Setup instructions for each client
are in [docs/CLAUDE_PLUGIN.md](docs/CLAUDE_PLUGIN.md). Website:
[kintsugi.tools](https://kintsugi.tools) + the
[paper landing page](https://kintsugi.tools/paperscope.html).

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

CJK (Chinese/Japanese/Korean), Thai, Lao, Khmer and Myanmar are indexed as **overlapping bigrams + first character** — a dictionary-free scheme that needs no 15–20 MB word list and keeps two-character words searchable (which `trigram` cannot). NFKC normalization handles full-width/half-width forms.

The scheme is strongest for Chinese and Japanese, where words are not space-separated. **Korean is weaker**: Hangul is written with spaces between words, and bigrams match character pairs rather than whole words, so a query for a whole eojeol only matches when its bigrams appear. Expect lower precision on Korean than on Chinese/Japanese.

Optional dictionary segmenters behind features: `jieba-rs` (Chinese), `icu_segmenter` (ICU4X word boundaries for CJK, Korean, Thai, Lao, Khmer, Myanmar, Arabic, Hebrew, Devanagari and more), and `lindera` (Japanese IPADIC morphological analysis in search mode, dictionary embedded in the binary). All are default off to keep the binary small.

## Build options

| Feature | Default | Effect |
|---------|---------|--------|
| `embeddings` | on | Candle + tokenizers: semantic search and reranking. |
| `daemon` | on | `paper daemon` background indexer + `paper daemon-control` (systemd/launchd service). |
| `jieba-rs` | off | Chinese word segmentation instead of bigrams. |
| `icu_segmenter` | off | ICU4X word segmentation for all major scripts. |
| `lindera` | off | Japanese IPADIC segmentation (embedded dictionary, no network). |
| `shell-accel` | off | Use `pdftotext`/`pandoc` when present, pure Rust otherwise. |
| `ocr` | off | Tesseract OCR for scanned pages. |

```bash
cargo install paperscope --no-default-features   # keyword-only: 11.8 MB vs 15.1 MB
```

A keyword-only build has no inference code in the binary at all; `paper search`
still works, and `--semantic` explains how to get the full build.

## Models

| Model | Size | Default | Purpose |
|-------|------|---------|---------|
| `BAAI/bge-small-en-v1.5` | 134 MB | Yes | English semantic search (384 dims) |
| `intfloat/multilingual-e5-small` | 487 MB | Opt-in | 100 languages (384 dims) |
| `BAAI/bge-reranker-base` | 1.13 GB | Opt-in | Cross-encoder reranking |

All models are Hugging Face Candle (pure Rust, no ONNX/Python), commit-pinned with per-file SHA256. Downloads come from this repo's GitHub Releases (resumable, SHA256-verified) with the pinned Hugging Face revision as fallback.

## Answers

Answers are **extractive**: a verbatim quote from a local document plus exact citations. Without `--polish` nothing generative is ever involved.

`--polish` optionally rewrites that extract through an engine you configure:

```toml
# $PAPER_HOME/config.toml
[local_engine]
base_url = "http://localhost:11434"   # Ollama, LM Studio, vLLM, llama.cpp
model = "llama3.1"
```

```bash
paper answer "when does the fridge warranty expire" --polish --engine local
```

The rewrite is **grounded and checked**: the extracts are wrapped in delimiters (delimiters inside the text are escaped), the engine must cite every claim as `[doc:N]`, and a reply that cites an extract that does not exist — or cites nothing — is rejected and the verbatim extract is used instead. The local citations always stay attached, so generated prose can be checked against the source.

`--engine openrouter` sends the extracts to OpenRouter. It is opt-in: it needs `--polish --engine openrouter` **and** an API key (`OPENROUTER_API_KEY` or `[openrouter] api_key`). `PAPER_OFFLINE=1` refuses to contact any engine.

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