# Paper

![Paper, local-first AI document search with exact citations](https://raw.githubusercontent.com/kintsugi-tools/paperscope/main/docs/assets/banner.svg)

[![crates.io](https://img.shields.io/crates/v/paperscope.svg)](https://crates.io/crates/paperscope)
[![npm](https://img.shields.io/npm/v/paperscope.svg)](https://www.npmjs.com/package/paperscope)
[![downloads](https://img.shields.io/crates/d/paperscope.svg)](https://crates.io/crates/paperscope)
[![M8ven Score](https://m8ven.ai/badge/mcp/kintsugi-tools-paperscope-mv88sq?v=fe587d3f0bf2c46c74cf5815e8f6dd24)](https://m8ven.ai/mcp/kintsugi-tools-paperscope-mv88sq?s=readme)
[![Licence](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue)](#licence)

**Local-first AI document search with exact page and line citations.** Ask in
plain language, get the answer *and* the file, page and line it came from. One
binary: a CLI and an MCP server for Claude and other agents. No account, no API
keys, no telemetry, and it works offline.

Latest release: **0.3.1**

![Animated demo](https://raw.githubusercontent.com/kintsugi-tools/paperscope/main/docs/assets/demo.svg)

## Install

```bash
cargo install paperscope        # installs the `paper` binary (Rust 1.94+)
```

Or the prebuilt binary:

```bash
curl -fsSL https://kintsugi.tools/paper.sh | sh
brew install kintsugi-tools/tap/paper
npm i -g paperscope             # any OS with Node 14.18+; fetches the same binary
```

The first `paper` command finishes setup by itself: it registers a background
indexer (launchd / systemd), downloads the 134 MB embedding model and then the
1.13 GB reranker, and indexes `~/Documents`, `~/Downloads`, `~/Desktop`.
Keyword search works from the first second, semantic results appear when the
embedder lands, and ranking sharpens once the reranker finishes. Opt out with
`PAPER_NO_AUTOSTART=1`, skip the reranker with `PAPER_NO_RERANKER=1`, and remove
the service with `paper daemon-control uninstall`.

## Try it

```bash
$ paper search "when does the fridge warranty expire"
/Users/you/Documents/fridge-warranty.txt:1-4
  Samsung refrigerator warranty certificate … Coverage ends on 2027-01-15.
  1.000 · matched by keyword search
```

```bash
paper answer "what did we agree about the deposit?"   # quotes, then a short answer
paper aggregate --field amount --op sum --filter kind=invoice
paper daemon-control status
```

Every result carries `path` + `line_start` + `line_end`, so a citation always
resolves to the lines it quotes. Answers are extractive: sources first, then one
or two sentences, and "not found" when the evidence is weak.

## Why it is different

- **Citations, not hallucinations.** Hybrid FTS5 + `sqlite-vec` with rank
  fusion, plus abstention on low-confidence matches.
- **Private by construction.** The index and embeddings stay on your machine;
  `PAPER_OFFLINE=1` forbids all network access.
- **Agent-ready.** MCP tools: `search_documents`, `read_source`, `answer_documents`,
  `index_documents`, `index_status`, `add_index_root`, `remove_index_root`.
  Path vetting keeps prompt injections away from `.ssh`, `.aws`, `.kube` and
  similar directories.
- **Broad input.** PDF (exact page), DOCX (paragraph ranges), Markdown, TXT,
  HTML, CSV, JSON, EPUB, RTF, ODT, XLSX, PPTX, and PNG/JPEG/TIFF/WebP/BMP/GIF
  images.
- **Scans are searchable.** OCR runs automatically on pages with no text layer
  and on standalone images, no Tesseract, no system tools, no setup. Text from
  a scan is cited as `page 12 (OCR)` so a citation never hides its origin.
- **100+ languages** with `multilingual-e5-small`, including cross-lingual
  search.

## Models

| Model | Size | Use |
|---|---|---|
| `bge-small-en-v1.5` | 134 MB | default English embeddings (auto-downloaded) |
| `multilingual-e5-small` | 487 MB | 100 languages |
| `bge-reranker-base` | 1.13 GB | reranks top hits (auto-downloaded; `PAPER_NO_RERANKER=1` to skip) |
| PP-OCRv4 det + rec | 16 MB | reads scans and images (auto-downloaded; `--no-ocr` to skip) |

```bash
paper models list
paper models install multilingual-e5-small
paper update --check        # is there a newer release?
paper update                # install it
```

## Links

- [GitHub repository](https://github.com/kintsugi-tools/paperscope)
- [Website](https://kintsugi.tools/paperscope.html)
- [crates.io](https://crates.io/crates/paperscope) · [npm](https://www.npmjs.com/package/paperscope)
- [Changelog](https://github.com/kintsugi-tools/paperscope/blob/main/CHANGELOG.md)
- [Implementation status](https://github.com/kintsugi-tools/paperscope/blob/main/docs/implementation-status.md)
- [MCP setup](https://github.com/kintsugi-tools/paperscope/blob/main/docs/CLAUDE_PLUGIN.md)
- [M8ven score](https://m8ven.ai/mcp/kintsugi-tools-paperscope-mv88sq?s=readme)
- Claude Code: `/plugin marketplace add kintsugi-tools/paperscope` then
  `/plugin install paperscope@paperscope`

## Licence

MIT OR Apache-2.0.
