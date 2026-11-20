# Changelog

All notable changes to this project are recorded here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html); while the version is
`0.x`, minor releases may change behaviour.

## [0.1.0] — 2026-11-19

First published release.

### Added

- `paper index` — incremental indexing of UTF-8 text files into one SQLite file,
  with line-accurate, overlapping chunks (`blake3` + mtime + size skip).
- `paper search` — four-tier retrieval: FTS5 keyword search (`bm25`, `OR`
  semantics), sqlite-vec cosine KNN with a relevance floor, reciprocal-rank
  fusion (k=60), and optional cross-encoder reranking.
- `paper answer` — extractive answers: a verbatim quote plus its citation, never
  generated prose.
- `paper models list|install|remove` — model download with pinned commit and
  per-file SHA256 verification.
- `paper serve --mcp` — MCP stdio server exposing `search_documents`,
  `read_source`, `list_sources` and `answer_documents` with a root allow-list.
- `paper doctor` — environment and index diagnostics.
- Exit codes: `0` found, `1` not found, `2` error, `3` model missing.
- `PAPER_HOME` (data directory override) and `PAPER_OFFLINE=1` (no network).

### Models

- `BAAI/bge-small-en-v1.5` — default embedding model, 134 MB, 384 dims, MIT.
- `intfloat/multilingual-e5-small` — opt-in multilingual embeddings, 487 MB,
  384 dims, MIT.
- `BAAI/bge-reranker-base` — opt-in cross-encoder reranker, 1.13 GB, MIT.

All three are ungated, pinned by commit and per-file SHA256, and run through
Hugging Face Candle (pure Rust). Searching never downloads a model.

### Known gaps

- PDF, DOCX, spreadsheet and email extraction are not implemented yet; only
  UTF-8 text formats are indexed and `page` is always 0.
- `paper index --watch` prints a notice and exits.
- `--polish` prints a notice and returns the verbatim extract; no generative
  engine is bundled.
- `--json` and text output are not yet a stability guarantee.
